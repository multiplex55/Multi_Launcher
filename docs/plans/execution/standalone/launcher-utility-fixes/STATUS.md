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
| M1-A | A: root query and intentional focus transfer | implemented/tested; native limited |
| M2-A | B: root-results-local fade override | implemented/tested; native limited |
| M3-A | C: release profile and bounded feature audit | complete |
| M3-B | C: idle capability work and ordinary release build | complete |
| M3-R | Independent A-C review/report/GitHub publication | complete; published |
| M4-A | D: recovery persistence before scratch close | implemented/tested; native scratch close/reopen verified |
| M5-A | E: accepted viewer-only preview dragging | implemented/tested; native limited |
| M6-A | F: top-level pending decision controls | implemented/tested; native limited |
| M7-A | G: bounded details, typed cached sort/filter, reveal | implemented/tested; native limited |
| M7-B | G: bounded directory/path capacity | complete |
| M8-A | H: actual Shell baseline observation correction | implemented/tested; native observer verified; ordered tabs limited |
| M9-A | Final independent review, scoped Clippy, exact-tip handoff | delivered LIMITED; external native acceptance open |

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

Verification: RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E with exact selector below. 15/15 passed, 7,673 skipped, 9.363s execution, run 6d2c6a33-45bf-4a96-8ffa-30f237ad2bc2. Existing owned-process wrapper: 100s execution deadline excluding separately completed compilation; 11.467s actual, exit 0, cleanup and output drain verified. Receipts target/launcher-utility-fixes/evidence/m1a-focused/{result.json,stderr.log}. Native verification pending consolidated release smoke after C; these are egui/state tests, not Windows UI proof.

```text
test(studio_usability_query_focus_) | test(studio_usability_diagram_viewer_embedded_fallback_keeps_root_host_identity) | test(notes_query_mouse_) | test(notes_query_hint_focuses_actual_query_and_returns_to_quick_notes_draft) | test(jump_mode_actual_query_capture_preserves_text_and_survives_discarded_pass) | test(notes_query_activation_mouse_workflow_runs_current_result_beside_notes) | test(notes_query_activation_autocomplete_tab_keeps_focus_through_discarded_pass) | test(launcher_show_focus_waits_for_root_focus_and_never_refocuses_an_already_visible_query) | test(launcher_show_focus_cancels_on_click_tab_and_protected_owner_takeover) | test(launcher_show_focus_rejects_replaced_root_generation)
```

M1-A commit 6ec61e5b1d765022ee06381cd419d6acd15455b8; ordinary git push succeeded; clean checkpoint tree. M2-A planning active.

## M2-A result

The existing root-results UI saves its fade strength, sets scroll.fade.strength=0 immediately before the shared list/grid ScrollArea, and restores it immediately afterward. No new child UI/widget identity, global/vendor styling, selection-scroll timing or geometry changes. Parent inspected exact scoped diff; git diff --check passed.

RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection) | test(root_grid_viewport_builds_complete_bounded_rows_with_absolute_ids_and_click_targets)': 2/2 passed, 7,686 skipped, 2.962s execution, run 12b46689-66d7-4b5d-a08c-17e96e77db1d. Separate compilation 1m20s; existing 100s owned runner exit 0 in 4.986s, cleanup/output drain verified. Receipts target/launcher-utility-fixes/evidence/m2a-root-results/{result.json,stderr.log}. Native visual check pending C release smoke; no evidence supports an extra selection-scroll change.

M2-A commit b5df0946752e10d597bc76f01d84df8fa83d754a; normal push succeeded, clean checkpoint tree. M3-A planning active.

## M3-A result

Cargo.toml directly defines release opt-level=3, lto=thin, codegen-units=16, strip=symbols, panic=unwind. iteration adds strip=none; existing opt2/cgu64/ltooff/incremental and effective debug/panic fields preserved; dev/fast-dev untouched. Parent inspected manifest diff; Cargo.lock unchanged. Thin LTO may increase build/link work; no benchmark/performance metrics claimed.

Bounded feature audit: cargo tree -e features -i eframe; cargo tree -e features -i image@0.24.9; cargo tree -e features -i image@0.25.6 all succeeded. No safe removal identified. Glow explicitly selected in main.rs4515; image024 serves screenshot/crop/diagram PNG/JPEG (screenshots also enables defaults), image025 serves egui_commonmark/extras and notes_markdown/html.rs541 decoder dispatch. Keep intentional decoder sets, accessibility/fonts/platform/link/web support; production catch_unwind in main.rs7191 and diagram/rendering.rs1488 requires unwind. No feature/version migration.

cargo metadata --no-deps --format-version 1 parsed manifest; profile-only TOML comparison against HEAD confirmed effective iteration settings preserved; git diff --check passed. Initial image@0.25.10 audit query corrected to actual0.25.6. Python3.11 tomllib rejects existing Cargo-supported multiline inline tables, so comparison parsed profile sections only, with Cargo metadata validating the manifest. No behavior tests needed for unchanged dependencies. Ordinary release build pending M3-B.

M3-A commit 2224684e0cbadb5d33e1ab4624e7606eee150e01; normal push succeeded. M3-B planning active.

## M3-B result

Driver skips rules snapshot/capability allocation/path probes unless a shared engine admission-time predicate and lock-only watcher unleased-ready hint permit a possible attempt. The original executor repeats fresh admission and lease/rule/fingerprint/native-evidence validation. Probes stay outside the existing 30s lease; original captured admission Instant remains the rate/causal-output timestamp. 80ms cadence, atomic shutdown/timeout wake, pause/disable/cancel and snapshot publication unchanged. Expected benefit is fewer idle probes/allocations; no measured CPU claim. Parent inspected all three source files and diff; git diff --check passed.

RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E with selector below: 4/4 passed, 7,685 skipped, 6.359s execution, run48347404-bd13-4c2f-8d48-0e362ac74fc3. Existing 100s owned runner exit0 in8.548s, cleanup/output drain verified; target/launcher-utility-fixes/evidence/m3b-hotfolder/result.json. Initial extraction compilation found missing now uses (E0425); shared helper now returns the original Instant, preserving semantics; corrected compilation passed.

```text
test(idle_or_blocked_admission_does_not_construct_execution_capabilities) | test(m10e_driver_reuses_service_and_restores_enabled_intent_without_backfill) | test(m10f_disabled_rule_requires_strong_enable_and_moves_only_new_settled_files_across_lifecycle) | test(m10c_fake_ready_execution_dedupes_partial_outputs_and_caps_runs)
```

Single ordinary cargo build --release succeeded exit0 in13m10s on this Windows host. Log target/launcher-utility-fixes/evidence/m3b-release-build.log. Executable target/release/multi_launcher.exe, SHA256 64D2D0DD61E7BBA50D2EDAD7DA73D29FB0B0044F8E1F442E6A461EC434275AEC. Six first-party library warnings are in unchanged Note Panel/Table Utility code; vendor warnings remain outside scope. No second release build.

## A-C native smoke evidence and limitations

- Existing owned-process runner launched the exact release executable with isolated working directory/settings/actions and ML_NOTES_DIR pointing at disposable notes. Production profiles/user notes were not edited. Paste Sequence next/cancel shortcuts were disabled before application construction.
- Initial sandbox process had a responding native main-window handle but was absent from helper inventory. Exact task-owned PID/path/start-time matched before retirement; runner exit-1 after129.838s, cleanup/output drain verified. A bounded authorized interactive launch then exposed the intended window successfully.
- Interactive release smoke: native keyboard navigation focused the real query; literal text entry, note search, selection and Return opened Quick Notes containing the disposable fixture. Accessibility tree saved at target/launcher-utility-fixes/evidence/native-abc-interactive/accessibility-notes.txt. Alt+F4 exited the app normally, runner exit0 after525.913s, cleanup/output drain verified; subsequent process and helper inventories contain no task launcher.
- Screenshot capture failed twice with window capture timed out: timed out waiting on channel, including one refreshed/re-activated target retry. An indexed query click failed coordinate input geometry is unavailable. No further screenshot backend recreation, alternate helper, guessed coordinate or privacy/security change was attempted. Accessibility reads and keyboard input worked; Notes accessibility set_value produced no observed edit and is not counted as success.
- Therefore A's real query-click round trip with multiple detached editors, and B's temporal fade/flicker/wheel/context-menu visual checks, remain native-unverified. Actual-frame focused tests passed; they do not replace this native evidence. A supported capture/input-geometry session is the smallest remaining verification requirement. These limitations do not stop D-H.
- Disposable note SHA256 remained C15A673AB6067A9371EDEC3C6DA20D343A1763CA97E859B9AF7D36726D896A04. No user process was killed and no source fixture was lost. Native session receipts are native-abc/result.json and native-abc-interactive/result.json under the task evidence directory.

M3-B commit 2c01ab4fa871521b93065c6bf9622e4fe06ca55a; ordinary push succeeded and checkpoint tree was clean. abc_tip_sha=2c01ab4fa871521b93065c6bf9622e4fe06ca55a. M3-R independent reviewer is inspecting baseline c10da8ddd90d3ebc7cf477409deb7921921a10d8 through this code tip. GitHub connector search still found no open PR for this head; create one full A-H draft after report commit. D-H remain active and will start immediately after publication; no approval/merge wait.

## M3-R review correction

Automated independent reviewer inspected baseline..2c01ab4f and found one P2: plain ROOT with no open tools returned no ordinary context, preventing a real query click from staging active Jump cancellation; detached Markdown in the regression masked the case. Planner confirmed stable empty-layer context is the correct owner model. Correction keeps that context before/after cancellation without relaxing blockers, native identity, retained-pass or geometry checks. Existing real-click regression now covers bare ROOT (no tools asserted) and detached Markdown, discarded press rejection, fresh click, Unicode and durable later focus. Parent inspected the corrective diff; git diff --check passed.

RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(studio_usability_query_focus_) | test(notes_query_mouse_note_panel_round_trip_and_obstruction_refusal) | test(launcher_show_focus_cancels_on_click_tab_and_protected_owner_takeover) | test(launcher_show_focus_rejects_replaced_root_generation)': 8/8 passed, 8.485s execution, run5f6bbcba-8152-4463-8e63-2ffc52ec255a. Existing100s owned runner exit0, cleanup/output drain verified; evidence target/launcher-utility-fixes/evidence/m3r-root-focus/. Comment-only cleanup followed passing source checks.

The successful release artifact/native smoke belongs to pre-correction tip2c01ab4f, not this later query correction. Its C profile/HotFolder evidence remains valid; corrected root logic is covered by focused fast-dev tests. No second release build or repeated unavailable screenshot environment was required/claimed. Final reviewer follow-up and exact abc_reviewed_tip_sha follow the corrective commit.

Live remote ancestry audit via authorized git ls-remote confirms predecessor16b740fec66a3c077b63ac22a31cfb9577896e65, integration master7af8148a20559e8bd2b488dbd0510c4630b2d0fd and pre-correction task2c01ab4fa871521b93065c6bf9622e4fe06ca55a, all matching local records.

## M3-R review accepted for publication

Correction commit8f4db7e10004e68bfc547832614adbc49fcee125 pushed successfully. Independent follow-up confirmed the sole P2 resolved, no additional concrete findings, no reviewer edits/tests. abc_reviewed_tip_sha=8f4db7e10004e68bfc547832614adbc49fcee125. Automated report PEER_REVIEW_ABC.md records scope, disposition, actual checks and native/release-artifact limitations. Report/status commit and GitHub publication follow; D-H remain active, with no approval/merge wait.

## M3-R publication receipt / immediate D continuation

Report/status commit994d8a6f2f41f44ed762678e70b312d2b23daaf4 pushed successfully. Final reviewed code remains8f4db7e10004e68bfc547832614adbc49fcee125. GitHub connector created one draft PR for the complete A-H Goal against master, and successfully posted the automated independent review summary (comment ID6722930334).

- PR: https://github.com/multiplex55/Multi_Launcher/pull/1465
- Review comment: https://github.com/multiplex55/Multi_Launcher/pull/1465#issuecomment-6722930334
- Commit-pinned report: https://github.com/multiplex55/Multi_Launcher/blob/994d8a6f2f41f44ed762678e70b312d2b23daaf4/docs/plans/execution/standalone/launcher-utility-fixes/PEER_REVIEW_ABC.md

Publication succeeded; publication_pending=false. No human reviewer was assigned, no PR merged and no approval/CI wait introduced. M4-A/taskD planning started immediately on the same branch, followed by E-H and final focused review. The report's native limitations remain open.

## M4-A / D result

Accepted same-frame edits/history are applied before close. Remembered unbound scratch now calls acknowledged synchronous recovery save before the session's explicit close_recovered_scratch retirement; the helper rejects saved documents and pending transitions. It clears the session/history without invoking Discard or manufacturing a disk baseline. Recovery failure leaves source/session/open state and visible error intact; existing native CancelClose runs. The successful scratch path avoids duplicate post-close writes. Saved-document prompts/conflicts, Remember-off, New/Open and companion retirement are preserved. Parent inspected the two-source-file diff; git diff --check passed.

Tests cover latest same-frame Unicode/newline edit and exact reopen, recovery failure/native CancelClose/draft retention, session guard rejection, and dirty saved-document conflicts with Remember enabled. The failure fixture acknowledges/flushed pending recovery before replacing only its disposable file with a directory, avoiding debounce races. Initial selection was7/8 because the saved-document fixture enabled Remember before explicitly opening the saved document and instead restored scratch; corrected by enabling Remember after the explicit open, preserving the intended bound-document scenario.

RUST_MIN_STACK=16777216; standard fast-dev lib Nextest prefix with selector below passed8/8, 7,684 skipped, execution0.548s, run23bb0938-a3e9-46a5-ba13-21668f95bfa7. Owned100s runner exit0 after2.776s, cleanup/output drain verified. After failure-fixture-only hardening, exact remembered_scratch_failed_recovery_cancels_native_close passed1/1, execution0.476s, run2e6f0d6b-6cf4-40a0-8f98-17500cd7a99f; runner exit0 after2.603s, cleanup/output drain verified. Separate compilation passed. Receipts target/launcher-utility-fixes/evidence/m4a-pad-close-corrected/ and m4a-pad-failure-fixture/.

```text
test(remembered_scratch_) | test(recovery_scratch_close_rejects_saved_or_pending_documents) | test(accepted_transition_intents_preserve_or_clear_recovery_at_lifecycle_boundaries) | test(save_and_close_follow_an_accepted_same_frame_source_edit) | test(saved_document_reopens_and_dirty_close_write_conflict_keeps_the_draft) | test(native_popout_child_discard_and_os_close_keep_the_edited_session) | test(new_and_close_transitions_handle_save_discard_and_cancel)
```

No release rebuild. Native D close/reopen remains pending feasible consolidated later smoke; current evidence is state/egui/filesystem persistence, not observed OS interaction. M3-R comment URL was independently fetched from GitHub and verified to match the successful publication receipt above.

M4-A commit 2064b428783aefa06e2ed084f7fa602847d84181; ordinary push succeeded and checkpoint tree was clean. M5-A/E planning is active.

## M5-A / E implementation decision

All viewer render modes now reuse their actual preview response for drag intent; accepted viewer effects remain the authorization boundary. The existing embedded viewport fallback owns its egui Window internally and ignores builder position updates. Public AreaState load/position setters lacked a storage API, so the scoped solution adds AreaState::store in vendor/egui/src/containers/area.rs as a thin wrapper over the existing memory owner. This avoids duplicate embedded windows, magic private widget IDs or a parallel window manager. The five-line vendor API change is necessary for actual accepted docked/embedded movement; no unrelated vendor changes or broad validation are planned. Native movement remains explicit viewer-only StartDrag. Focused implementation checks completed as recorded below; physical native drag remains unverified.

## M5-A / E result

Mermaid native/generic and Visual viewer previews reuse their existing canvas responses to collect primary drag, including placeholder/error surfaces. Visual minimap press origins are excluded. PendingViewerEffects carries the gesture; current retained ROOT/child pass, owner/viewer generation and host identity are required. Native drag starts only on viewer_viewport_id; ROOT-hosted docked/embedded presentations update their actual accepted viewer AreaState using the narrow store API. Close/host-change/stale/discarded effects do not move a viewer. Cameras/document revisions, editor behavior, middle pan, minimap and toolbar/navigation remain preserved. Parent inspected source/vendor/tests; git diff --check passed. No private widget IDs, duplicate windows, raw Windows APIs or production dependencies added.

RUST_MIN_STACK=16777216; standard fast-dev lib Nextest prefix with selector below passed8/8, execution0.797s, run400442c5-ce2d-4c46-84a1-d61e056d1e70; owned100s runner exit0 after2.981s, cleanup/output drain verified. Initial5/5 passed before adding negative/control cases. A final explicit no-ROOT-StartDrag assertion for actual docked/embedded movement passed its exact case1/1, execution0.281s, run98dba42b-0489-48c3-b9b4-d0178262d96d; runner exit0 after2.380s, cleanup/output drain verified. Separate compilation succeeded, including egui consumers. Receipts target/launcher-utility-fixes/evidence/m5a-viewer-drag-focused/ and m5a-root-command/.

```text
test(diagram_viewer_preview_drag_) | test(studio_usability_diagram_viewer_discard_close_and_reopen_are_generation_scoped) | test(studio_usability_diagram_viewer_embedded_fallback_keeps_root_host_identity) | test(diagram_studio_visual_canvas_viewer_navigation_keeps_editor_camera_independent) | test(visual_minimap_drag_keeps_press_mapping_and_camera_independence)
```

Native physical preview drag is unverified due the established capture/coordinate-geometry limitation; model/egui command/position checks are not claimed as OS proof. No additional failed capture environment or release build was recreated.

M5-A commit 803e8150f386bbc23571fb5d953d617fff70ed1a; ordinary push succeeded, checkpoint tree clean. M6-A/F planning active.

## M6-A / F result

One shared draw_pending_decision renderer runs at draw_body entry, immediately after host presentation controls and before either Mermaid/Visual scroll body. Removed both old in-scroll panels. Existing ResolvePending intents, accepted effects, save/conflict/cancel behavior and Mermaid popup ownership guard remain; wrapped controls and test hooks retained. Search confirms a single production decision renderer. docs/DIAGRAM_STUDIO.md now documents viewer preview window dragging and the pinned decision location. Parent inspected source/test/guide diffs; git diff --check passed.

Focused fast-dev lib Nextest selection below passed4/4, execution0.582s, runc73f5239-756c-4d4f-8e61-95f2b7c79347. RUST_MIN_STACK=16777216; separate compilation and existing100s owned execution, runner exit0 after2.860s, cleanup/output drain verified. Layout test checks Mermaid/Visual × native/docked, real stored body scroll offset, 1.25x fonts, exactly one Save/Discard/Cancel set, clipping/position above the body and actual Cancel preserving dirty state. Existing transition/save/canceled-chooser/discarded-pass assertions retained. Initial test-only style API compile error corrected; initial3/4 run exposed unreliable wheel targeting in the fixture, replaced by an observation hook for the existing ScrollArea ID and direct stored offset, with no production ID change. Receipts target/launcher-utility-fixes/evidence/m6a-decision-panel-state/.

```text
test(diagram_studio_pending_decision_stays_above_scrolled_body) | test(diagram_studio_close_decision_orders_edits_and_explicit_reopen_starts_blank) | test(diagram_studio_os_viewport_close_requests_cancel_until_dirty_decision) | test(diagram_studio_visual_template_discarded_root_and_decisions_preserve_state)
```

Native visual acceptance remains unverified because capture/coordinate geometry is unavailable. Feasible later keyboard smoke will be consolidated after remaining UI changes rather than repeatedly rebuilding/launching. No release rebuild or additional capture retry.

M6-A commit a6d4c553f8be79021ce5e6a9697ae372914e96fe; ordinary push succeeded, checkpoint tree clean. M7-A/G usability planning active; capacity remains the separate M7-B checkpoint.

## M7-A / G usability result

Status/subtotal/counts and errors remain visible; default-collapsed Scan details contains finite-height independent scrolling and all domain-bounded issue examples (64 rather than silent UI truncation to8). Typed Name/Type/Size/Share/Status sorting supports both directions with active arrows, deterministic name/index ties and unknown measurements last; zero-denominator shares tie deterministically. All/Files/Folders preserves omitted kinds in All and never changes scan totals. Current-directory original indices are cached by projection publication/directory/sort/filter, with virtualization and stable generation/index row identity. Hidden selection clears; Up and no-match state remain available. Obsolete test-only sorter/visibility helpers were removed and tests now exercise the real path.

Right-click selects its exact original row; a typed Reveal request carries generation/publication/index/directory and originating retained host/epoch through RootDialogEffect. Final dispatch re-resolves it against current open/session/projection/root/visibility/selection before try_exists and native calls, preserving access errors and reporting missing/pathless targets. Root paths use typed OpenRoot routing; normal entries use shared reveal_path, separate from existing Open. Menu eligibility is scoped to actual popup ID, host/epoch, generation, directory and visible row across publications, while final actions retain exact-publication checks. Discarded openings are cleaned up and discarded Reveal cannot suppress a later fresh action. Foreign popup/modal/foreground guards remain. Origin-host stamping prevents a host-changing batch from borrowing new authority. README Folder Size instructions updated; no scanner limits changed here.

RUST_MIN_STACK=16777216; standard fast-dev lib Nextest prefix with selector below passed6/6, execution0.987s, run51f96722-f903-4c39-9287-05aa53fc177c. Existing100s owned runner exit0 after3.128s, cleanup/output drain verified. Initial5/6 exposed the real own-popup host lock; corrected at the ownership boundary. An accidental removal of shared row measurement during obsolete-helper deletion caused E0425 compile feedback and was restored before passing checks. Final originating-host assertion reran the exact actual-row case1/1, execution0.872s, run529240fe-dac9-4571-af1f-b7eea91b7a73; runner exit0 after3.179s with cleanup/output drain verified. Separate compilation passed. Evidence m7a-folder-size-final/ and m7a-origin-host/ under task evidence. Parent inspected source, popup lifetime, dispatch and README diffs; git diff --check and stale-reference search passed.

```text
test(folder_size_dialog_projection_sorts_drills_and_rejects_stale_generation_selection) | test(folder_size_dialog_sorted_filtered_context_reveal_keeps_original_row) | test(folder_size_dialog_reveal_root_route_is_explicit) | test(folder_size_dialog_close_wins_same_pass_arbitration) | test(folder_size_retained_close_suppresses_same_frame_prefill_focus) | test(folder_size_native_owner_rejects_discarded_choose_scan_copy_and_close)
```

Coverage includes all comparator directions/unknowns/filter totals, cache reuse/invalidation, bounded long-issue disclosure, actual sort/filter then selecting a different right-click row, discarded/fresh Reveal, aged-publication menu ownership, host transition rejection/fresh request, missing/hidden/refresh/close rejection and root routing. Native Explorer selection was not observed and is not claimed from dispatch. A tiny disposable final-smoke tree was prepared under target/launcher-utility-fixes/native-fixtures/folder-size; no user drive scan. No release rebuild or capture retry.

M7-A commit bc8d79e5e1ac5b69bfa2c455af55875836eb670e; ordinary push succeeded and checkpoint tree clean. M7-B/G capacity planning active.

## M7-B / G capacity result

Folder Size directory capacity is16,384 in both accumulator admission and supplied traversal bounds. Aggregate scan path accounting is64 MiB: discovery32, result8, cumulative immutable progress8, scanner/accumulator/converted issues4 each, root/bookkeeping4. Other entry/depth/issue/message bounds remain100,000/64/64 and unchanged message bounds. Accounting is conservative/cumulative for handle paths, not total RSS or GUI allocations. Queued frames share ancestor chains; handles retire as frames/chains drop on completion/cancellation and are absent from final results. Shared scanner production/defaults unchanged. A scan-owned bounds constructor feeds both runtime and fake-backend verification via a test-only re-export, avoiding duplicate test budget definitions. README now states directory/entry/depth bounds and truthful partial results.

RUST_MIN_STACK=16777216; cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast --test-threads 1 -E with selector below passed8/8, execution2.529s, runaabfe50f-2ced-422e-a325-a1b328b4d371. Existing100s owned runner exit0 after4.772s, cleanup/output drain verified; separate compile passed. Synthetic ordinary tree completes with2,223 admitted directories and subtotal41. Path-heavy fixture is Partial under old3 MiB discovery but Complete under the actual new32 MiB constructor. Small path/directory bounds preserve subtotal23 and exactly3 admitted directories. Cancellation after work keeps verified bytes and leaves zero fake handles. Existing policy/cloud/denial/progress/overflow cases pass. Evidence target/launcher-utility-fixes/evidence/m7b-capacity/. Parent inspected constants, accounting, handle/test changes and guide; git diff --check passed. No user drive scan, unrelated file-operation suite, production default changes or release rebuild.

```text
test(folder_size_scan_capacity_) | test(folder_size_scan_fake_metadata_preserves_root_subtotal_when_child_limit_hits) | test(folder_size_scan_fake_metadata_records_policy_exclusions_and_open_time_cloud_change) | test(folder_size_scan_fake_metadata_keeps_denied_and_changed_branches_incomplete) | test(folder_size_scan_progress_overlays_later_subtotals_and_cloud_omissions) | test(folder_size_scan_checked_totals_stop_before_ancestor_overflow)
```

M7-B commit f52435a6b04d6241f5621b1f1e19374319b00247; ordinary push succeeded and checkpoint tree clean. M8-A/H investigation is active. Native diagnostics must use the actual interactive context and retain the supported Windows capability gate.

## M8-A / H native diagnosis (active)

The opt-in ignored read-only observer test compiled in fast-dev and its exact binary test name was verified. Parent ran it in the elevated interactive desktop, with RUST_MIN_STACK=16777216 and EXPLORER_TABS_OBSERVER_RECEIPT pointing to task evidence. Both samples reproduced incomplete WindowIdentityUnavailable(index0), traced to initial/shell-browser-window after top-level HWND inventory and process lookup, HWND89459614; no HRESULT was supplied at that bool boundary. No Explorer window, saved-list request or target action was created. Execution0.177s, exit0, cleanup/output drain verified under the existing100s owned runner. Receipt target/launcher-utility-fixes/evidence/m8a-shell-observer.txt and m8a-observer-result.json. An initial parent runner invocation used the wrong Arguments parameter and did not launch; corrected ArgumentList invocation produced this evidence. Implementer is refining the actual relation failure before choosing a correction. Supported Windows version gate remains intact; this Windows10 host cannot establish ordered Windows11 tab acceptance. Final independent read-only D–G review is running in parallel with sequential H implementation; no concurrent Cargo jobs.

## M9-A incremental independent D–G review

Configured reviewer inspected committed D–G through f52435a6b04d6241f5621b1f1e19374319b00247, incorporating the existing A–C report. No actionable correctness/ownership finding. Reviewed recovery-before-retirement/failure retention; native versus ROOT viewer movement and pass identity; single top decision panel; typed/cached Folder Size ordering and final reveal revalidation; bounded path accounting/handle lifetime. Reviewer inspected passing final receipts, correctly distinguishing the failed initial decision-panel receipt from its corrected passing state receipt. No source edits or Cargo jobs. H remains excluded until its checkpoint; native limitations remain open.

M8-A refined interactive read-only receipt reproduced both samples at IOleWindow::GetWindow, HRESULT0x8001010D, before root comparisons. Microsoft COM error documentation names RPC_E_CANTCALLOUT_ININPUTSYNCCALL. No safe production mapping change has been selected. Opt-in diagnostic will inspect the active-view relation independently while clearly recording the failed browser relation, without making a production incomplete snapshot complete. Receipt m8a-shell-relation.txt, execution0.262s, exit0, cleanup/output drain verified. No Explorer windows created or targeted.

### M8-A bounded implementation plan: correct apartment ownership

Planner identified documented root cause: initialize_com_apartment selects MTA, but IOleWindow::GetWindow is input-synchronized and unavailable to MTA callers (Microsoft COM call-synchronization). Browser-to-view substitution would preserve the same defect. UIA guidance separately recommends a non-window MTA client thread. Preserve all browser/view/root/process checks and existing UIA MTA ownership.

Pending native STA proof, implement one operation-scoped persistent Shell STA owner containing observer, retained browser/view COM references, PIDLs, selected-view validation and BrowseObject navigation. Existing operation/UIA worker gets its own MTA guard. Bounded typed requests/results carry snapshots, diagnostics, WindowIdentity, pane handle value, FolderPath and binding tokens; no COM references/PIDL pointers cross threads. Migrate observer, retained_browser/current_folder_pidl/selected_view and all three BrowseObject paths; selected-view facade returns a token. STA navigation freshly revalidates target/pane/prior folder/binding immediately before mutation. Message-aware wait supports shutdown wake, cancellation/deadline checks surround calls, mutation-attempt reporting remains exact, references drop before CoUninitialize and worker retirement is explicit. Verify stale/canceled/deadline-before-mutation refusal, attempted-state receipt and shutdown lifecycle plus focused existing identity/bootstrap/operation tests. No sleeps, unsupported OS spoof, skipped baseline windows or broad subsystem rewrite. Sole implementer owns source; parent owns native receipts/Git. H supported-tab native acceptance remains unavailable on this OS even if observer proof passes.

Primary references: https://learn.microsoft.com/en-us/windows/win32/com/call-synchronization and https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading .

M8-A STA comparison preserved every existing relation check. MTA twice failed browser GetWindow0x8001010D. STA passed it and reached a second failure; refined receipt established QueryActiveShellView succeeds but its cast to IOleWindow fails E_NOINTERFACE0x80004002. Microsoft IUnknown_GetWindow documentation explicitly describes derived IShellView implementations that reject base-interface QueryInterface and uses direct inherited GetWindow. Planner verified local windows0.58 generated IShellView Deref to IOleWindow makes direct inherited invocation without QI. Selected correction uses inherited browser/view GetWindow plus unchanged returned-HWND/root/PID/TID checks at observation/recheck/selected-view sites, together with proper STA ownership. No check is removed. Receipts m8a-shell-apartments.txt (0.191s) and m8a-shell-view-stage.txt (0.375s), both exit0/cleanup/drain verified; incomplete snapshots truthfully remained incomplete. Reference https://learn.microsoft.com/en-us/windows/win32/api/shlwapi/nf-shlwapi-iunknown_getwindow .

M8-A full-check inherited-interface native proof succeeded: STA sample0 and sample1 each complete=true, windows201, views1, failure=None, final/snapshot-validation. MTA still reproduces0x8001010D twice. Receipt m8a-shell-inherited.txt; owned runner m8a-inherited-result.json, exit0, elapsed0.193s, cleanup/output drain verified. All original inventory, browser/view/root/process/thread, folder and recheck validations remained. This proves the two diagnosed observer causes; it is not an ordered-tab workflow proof. Sole writer now integrates the operation-scoped Shell STA owner with UIA MTA preserved and bounded typed requests/lifecycle. No gate spoof or Explorer creation.

M8-A integration review during implementation identified nested-port ownership and cancellation/wake races before acceptance. Sole writer replaced nested port with explicit ShellState; UIA remains MTA. Queued/running/retired atomic claim prevents canceled queued jobs from later mutating; running jobs deliver their receipt before reporting, including wake-failure handling. Pump drains messages before its final work-queue check. Existing synchronous native-call limitation remains: an already-running COM call must return before truthful attempt reporting/reference teardown, so no hard interruptibility claim is made. Integrated focused tests are compiling; no checkpoint acceptance yet. Parent holds all other Cargo jobs.


## M8-A / H checkpoint result

Source owner src/explorer_tabs/windows.rs now confines Shell observer, retained COM identities/PIDLs, selected-view validation and BrowseObject to explicit ShellState on one operation-scoped message-pumping STA. UIA and its elements remain on the operation MTA with their own guard. Typed facade exchanges snapshots/diagnostics/binding tokens/window and folder values; no COM reference/PIDL crosses the queue. Direct inherited browser/view GetWindow replaces redundant base QI while preserving all root/PID/TID/COM/folder and full baseline checks. Navigation freshly validates target/pane/prior folder/binding inside STA. Queue capacity1, explicit queued/running/retired claim, cancellation/deadline checks, accurate attempt markers, message wake and joined retirement. Diagnostics are bounded phase/stage/index/HWND and actual supplied HRESULT, without titles/paths or stale LastError.

Independent reviewer found one P2 migration issue: the public observer constructor still selected MTA after the port migration. Fixed normal connect to establish/require STA; deliberate MTA comparison is private cfg(test). Follow-up found no further concrete defects; public constructor and operation owner preserve COM guards/lifetimes. Targeted rustfmt edition2024 and parent diff/stale-path inspection passed; no unrelated files or dependencies changed.

Latest compilation succeeded. Standard fast-dev lib Nextest serial selector below passed8/8, run d06ed3b4-9370-43b6-b7d2-d04919d658b5, execution0.600s; previous integrated run967ed576-710e-4d18-83cb-2492ed713849 passed8/8 in0.675s. Existing100s owned runner exit0, cleanup/output drain verified. Evidence target/launcher-utility-fixes/evidence/m8a-owner-focused/. Tests cover owner dispatch/wake, cancellation before/during work, queued retirement/running receipt, deadline refusal and shutdown, existing unknown/reused identities, complete bootstrap exclusion, no retry after request, remaining-folder preservation and saved-list reload safety.

```text
test(explorer_tabs_shell_sta_owner_dispatch_cancel_and_shutdown) | test(explorer_tabs_identity_rejects_unknown_windows_and_duplicate_bindings) | test(explorer_tabs_identity_same_hwnd_with_changed_process_is_not_the_same_target) | test(explorer_tabs_backend_bootstrap_receipt_rejects_preexisting_and_ambiguous_windows) | test(explorer_tabs_backend_cancel_after_request_never_retries) | test(explorer_tabs_backend_bootstrap_only_failure_preserves_all_requested_folders) | test(explorer_tabs_operation_checks_cancellation_and_deadline_around_calls) | test(explorer_tabs_run_reloads_saved_id_and_refuses_deleted_or_corrupt_store)
```

Final reviewed binary native opt-in test through the real STA-owner queue passed two complete observations, each201windows/1view/no failure. Deliberate MTA comparison still reproduces0x8001010D twice, confirming the regression boundary. Final receipt m8a-shell-owner-final.txt, runner m8a-owner-final-result.json, exit0 elapsed0.202s cleanup/output drain verified. Earlier integrated receipt0.256s agrees. No Explorer creation/navigation, user-window retargeting or unsupported gate bypass occurred. This is native observer proof, not a demonstrated ordered-tab saved-list operation. Windows10 build19045 cannot provide the required Windows11≥22621 tab acceptance; it remains open. An already-running synchronous COM call can delay retirement; no unsafe abandoned worker or hard native-call interruptibility is claimed.

## M9-A final review/lint progress

M8-A commit7f301d5d92f707abac098eb7e63f8633ce0033d5; ordinary push succeeded and checkpoint tree clean. Final independent review report PEER_REVIEW_FINAL.md records D–G and H source boundary, corrected public-constructor P2 and no unresolved actionable finding.

Required first-party lint command `cargo clippy -p multi_launcher --profile fast-dev --lib --bin multi_launcher --no-deps` passed exit0 in2m10s. Log target/launcher-utility-fixes/evidence/m9-clippy.log. Existing library115 and binary3 warnings plus dependency/vendor warnings remain. Parent matched all warning locations in task-touched files against cumulative added-line ranges: none points to a Goal-added line. Examples are existing enum size/style warnings in Diagram Studio, existing collapsible conditionals in moved selected-view logic/Pad/Folder Size and pre-existing render references; unchanged main hotkey helpers have argument-count warnings. No unrelated fixes or warning suppression. Current `cargo build --profile fast-dev --bin multi_launcher` is running solely for the consolidated native keyboard smoke; no repeated release build.


## M9-A final build/native result and disposition

Current app `cargo build --profile fast-dev --bin multi_launcher` passed in4m20s, six existing warnings; log m9-fast-dev-build.log. Binary SHA25631166C7AB37977051788C903EA3E0CCA50C693325D08595685DFAF8574C5D145, source tip7f301d5d92f707abac098eb7e63f8633ce0033d5. This current build includes A–H; the earlier C release build boundary remains unchanged and no second release build was performed.

Isolated final profile had Paste Sequence next/cancel and launcher hotkeys disabled before construction, empty actions/index roots and test-owned notes/recovery. Actual keyboard query opened Pad; actual source native_smoke = 12 + 34 evaluated46. Keyboard Pop Out created a native Pad, native Alt+F4 closed it quietly, then query reopening restored exact source/result. Recovery JSON agrees. D native core close/reopen passes. With Pad detached again, root Tab navigation reached query, typed diagram studio and activated its result, creating a separate native Diagram Studio window. This is A keyboard evidence, not required mouse/hint acceptance. Native immediate child accessibility exposed only chrome, so no editable Diagram source, F native decision layout or E physical drag proof.

Folder Size opened and keyboard Choose folder opened native Select Folder. The picker appears in accessibility but its descendant actions are unavailable: Raise failed 'element122 has no cached secondary actions'; Set Value on fresh Folder field290 failed 'element290 is not available in cached app state'. Ctrl+L/Alt+D/Escape did not change it. No scan or Reveal was dispatched, so G native acceptance remains open. No guessed coordinates, alternate UIA/capture helper or retry of prior failed screenshot environment. Parent verified PID24332/path/starttime against recorded identity and retired only this test app to release its inaccessible owned picker. Runner exit-1 (explicit task cleanup, not app crash or timeout), elapsed442.559s, cleanup/output drain verified. Final sky/process inventory contains no launcher or Cargo job. Test note SHA unchanged71059A5EDDFC25881DA2E48CD56F8A2A33E3A67E3C03FE4D55FC2F2274160C18. Full observations/identity/artifact/runner receipts under target/launcher-utility-fixes/evidence/m9-native/. Empty disposable profile emitted missing optional radial/dashboard store messages; no user data edited.

Final disposition **LIMITED**, not full functional acceptance: A–H source implementation, targeted tests, ordinary C release build, current fast-dev build, scoped Clippy, independent reviews and GitHub interim delivery are done. D native scratch round trip and H native observer are verified. Still native-unverified: A genuine mouse handoff with detached editors, B fade/flicker/wheel/context visuals, E preview drag and preserved physical interactions, F top-panel native layout, G real scan/sort/filter/Explorer reveal, H supported Windows11 ordered saved-list tabs. Required next environment is working capture/input geometry plus Windows11 build≥22621 for tabs; no source/test review substitutes for those checks. An in-flight synchronous COM call can delay retirement until return.

Cumulative diff inspected; stale MTA public observer path removed, old decision copies/test sort helpers removed, current effect ownership paths used. `git diff baseline HEAD --check` passed; baseline immediate parent is exact16b740fec66a3c077b63ac22a31cfb9577896e65; no merge commits in parent..task range. No full suite, benchmark, worktree, auxiliary clone/checkout, intermediate branch or merge. All implementation checkpoints pushed normally. Final ledger/review/limited-handoff commit follows; retrieve its exact SHA with git rev-parse HEAD after commit. Final response and PR will contain actual final_tip_sha without a self-reference loop. The next Goal must create a fresh child from that exact final_tip_sha, not master/main.

## M9-B external PR-state reconciliation

Final GitHub metadata readback revealed PR1465 was externally merged at2026-11-08T15:17:16Z, through D/source2064b428783aefa06e2ed084f7fa602847d84181, merge commit30765ab331a0dfc854c8114d6ff4316f194b14b3. It is closed/merged and no longer draft. The agent did not merge it, wait for its merge or change the task branch. Work continued through E–H on the same branch as required. Normal final push succeeded before discovery; elevated read-only ls-remote confirms task brancha826849fa8a654131b5fb653489040dd4a98bd88 and master30765ab331a0dfc854c8114d6ff4316f194b14b3. Therefore the merged PR's recorded head does not include E–H or the final ledger, although those commits are pushed on the current task branch. No fetch/pull/rebase/merge/reset/branch switch or replacement PR was used to alter that external state. The requested single interim PR/comment/report publication remains delivered; final description is corrected to distinguish externally merged D from later branch commits. Initial final-description update had said draft before readback exposed the external merge; it is being corrected.

M9-A ledger commit a826849fa8a654131b5fb653489040dd4a98bd88 was pushed successfully; this new external fact justifies a small M9-B documentation follow-up. Capture final_tip_sha after that commit and use it for the next Goal; master currently ends at D and is not the required handoff. Native LIMITED disposition and source/test/build evidence remain unchanged; no recompilation needed for documentation-only reconciliation.
