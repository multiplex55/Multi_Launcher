# Studio and Launcher Usability execution

Contract: `docs/plans/CODEX_GOAL_STUDIO_LAUNCHER_USABILITY.md` and companion Kickstart.

## Ancestry and initial state

- Initial checkout: clean `master`, `1439676a2164e8e8ff4bd6bb9d9f2aee07515c25`; no user-dirty paths.
- Parent branch: `feat/visual-studios-06-markdown-studio`.
- Parent tip: `3b544af7507474f31131dea98bb8dea0756a4c4e`.
- Authority: complete predecessor handoff `target/visual-studios/program-final-handoff.json`, matching local branch and recorded upstream tip. Predecessor committed and pushed.
- Task branch: `feat/studio-launcher-usability`, initial HEAD verified equal to parent.
- Master differs only in documentation archival and the two new task contracts. Retain the task contracts on this exact-parent branch; do not import archival reorganization or merge master.
- One checkout; no worktrees, auxiliary clones, agent branches, or intermediate merges.
- Git branch creation initially failed because sandbox denies `.git/index.lock`; authorized escalation succeeded without changing user work.

## Checkpoints

| Checkpoint | State | Evidence |
| --- | --- | --- |
| M0-A | complete | Baseline `e5eef3a324f89d07dbff1eec5db1635a385ae980`; immediate parent equals required tip; initial `git push -u origin HEAD` succeeded |
| M1-A | complete | `b3073917a0b5f41d278c233a875ed1f69ae40084`, pushed; focused Nextest 6 passed |
| M1-B | partial | Core Enter workflow verified both modes (4/4 focused pass); Root-hint startup exhausted bounded corrections; see stop report |
| M2-A | complete | `46fba31e60446490ed210373d187544fc3b1a263`, pushed; final selection 8/8; native smoke M6 |
| M2-B | complete | Qualified pinned adapter; final ownership/publication/cache 5/5; parent commit next |
| M2-C / M2-D | pending | Templates / viewer depend on qualified renderer |
| M3-A / M3-B / M3-C | pending | Calculator grammar / host / recovery |
| M4-A / M4-B | pending | Note tables / safe close |
| M5-A | pending | Dashboard folds / save feedback |
| M6-A | pending | Scoped integration / native smoke / lint / independent review |

Use Goal section F checkpoint subjects and acceptance criteria. Planner supplies bounded packets; one implementer writes at a time; parent owns Git. Use cached fast-dev, nonempty focused Nextest selections with no-fail-fast, and new app fixtures disable both Paste Sequences shortcuts. One cumulative independent review and final lint pass. Native smoke is bounded to Goal H3. No full-suite campaign.

M0-A inspected intended staged paths (only the two contracts and this ledger), `git diff --check`, baseline SHA and direct-parent equality. No application builds/tests ran before bootstrap. Planner assigned read-only M1-A/M1-B source investigation after successful initial push. Source remains untouched at this point.

Planner packets and current source ownership findings are recorded in `PACKETS.md`; Goal F remains the acceptance contract. M1-A implementation distinguishes explicit query click from automatic show focus and current keyboard authority, using actual ordinary-owner layers while keeping unknown layers protected. No implementation test result claimed yet.

## M1-A verification

`cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(studio_usability_) | test(notes_query_mouse_quick_notes_round_trip_in_both_jump_modes) | test(notes_query_mouse_note_panel_round_trip_and_obstruction_refusal) | test(launcher_show_owner_native_capture_states_block_focus_permission)'` — PASS, 6 passed / 7,032 skipped, test execution 3.937s, build+test 5m42s. Earlier compiler feedback E0004 (incomplete Panel classification) and E0428 (duplicate new test definition) corrected before test execution; no behavior failure or remediation cycle consumed. Three new tests cover exhaustive tracked-owner classification and actual Settings/File Search/Calculation Pad/Dashboard Editor focus round trips in both Jump modes. Existing Notes obstruction and native-capture guards pass. New tests reuse isolated fixtures with Paste shortcut overrides. Windows native smoke remains M6; headless tests are not native-focus proof.

Parent inspected production diff and intended files; `git diff --check` passes. Source ownership changes are render.rs/mod.rs plus image-panel layer helper; Pad/Dashboard additions are cfg(test) response seams. Task-created unrelated rustfmt changes are being removed before commit. No broad suite/build performed.

M1-A committed `b3073917a0b5f41d278c233a875ed1f69ae40084`; normal `git push` succeeded, clean tree verified before B assignment. Content-identical playback formatting noise restored only after normalized blob equality with HEAD was checked. No unrelated source staged. Native capture was tested through shared policy, not an active-capture pointer roundtrip; hidden ROOT/foreign viewport/stale identity were preserved in code rather than directly rerun in this packet. M6 owns supported native acceptance.

## M1-B implementation / initial feedback

Routing changes are in render.rs plus narrow Jump UI chord introspection; obsolete presence-only Enter helper/tests removed in favor of production owner tests. Planner located watcher-free real NotePlugin fixture; matching today.md must exist on disk. `note list today` is synchronous, so no timing harness added.

Concrete investigation found active Root hint sessions would invalidate the accepted Mouse query session and return ownership to the neighboring tool. A narrow live Root target-generation/controller-owner path preserves that session, while query editing/workflow is disabled during leader/hint frames. This is not blanket active-hint permission.

Initial scoped B compile succeeded, 11 tests passed and the real-note test failed. Exact retry failed at retained-selection assertion. Root cause confirmed in fixture: query-owned Ctrl+Z correctly undid `note list today`, clearing/reverting results before activation. Correction will assert undo, re-enter query through actual TextEdit, verify restored real result, and use fresh navigation/activation; no expected-value weakening. Active-hint helper is also being tightened to actual Root controller ownership because HintSession itself is only a target map. B remains in progress; no acceptance claimed.

Further exact undo/retype exposed a genuine production cache defect: search.rs `search()` empty-query branch installs replacement results without retiring prior last_results_valid/last_search_query identity; restoring the previous nonempty string then hits a stale cache short circuit. Parent rejected a no-match intermediate-query fixture workaround; implementer confirmed source cause and canceled that in-flight workaround run (not a pass). Narrow cache invalidation at empty-result installation is directly required by full query editing, with existing uncached empty-browse behavior preserved. Exact undo/retype remains the regression; scoped empty-browse/cache and changed Root-hint checks will be included. No general search redesign authorized.

Root-hint bounded stop: [20261104T003517Z.md](stalls/20261104T003517Z.md). Latest completed exact three-test run compiled in 1m17s, 2 passed/1 failed/7,035 skipped. Dashboard-clear and empty-browse pass; real-note Jump-off Enter path executes successfully, Jump-on leader startup still drops Root targets/owner despite retained Mouse session. Two focused correction rounds exhausted for that behavior; no further Root-hint production changes/retries authorized under current budget. Failing test stays ordinary and meaningful. Separate test-only Enter-in-both-modes verification is allowed; independent checkpoints continue afterward. Future selections must be task-specific rather than broad `test(studio_usability_)` so they do not silently retry the exhausted known failure.

Separate core verification PASS (production Root-hint code frozen): `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(studio_usability_query_enter_activates_real_note_in_both_jump_modes) | test(ordinary_enter_cannot_activate_root_results_behind_timer_or_paste_dialogs) | test(studio_usability_query_clear_reaches_dashboard_without_losing_editor_draft) | test(track_c_metric_empty_browse_frame_completes_without_last_results_valid)'` — 4/4 passed, 7,035 skipped, compilation 3m02s. Test-only extraction separates fresh Enter in both Jump states from the normal still-failing Root-hint test; no ignored/deleted/softened hint assertions. Verified real stored note dispatch exactly once, unchanged dirty Pad, tool-owned Enter protection, real dashboard/draft retention, exact undo/retype after cache repair. No native proof or successful Root-hint result claimed. Parent final diff/whitespace inspection found only intended paths, no debug prints or unrelated formatting churn. Partial source is coherent/compiled but the known hint defect remains explicit; M1-B is not accepted complete.

Required reporting commit `0dd77735a869d48823f0f619071261d17717502e` contains only the human index and bounded-stop report; normal push succeeded. Partial source checkpoint follows separately and must not be described as M1-B acceptance.

Partial M1-B source preserved in `98b5d67723a4fc0de277eae66a491c1cbbb00c3d` (`fix(gui): [M1-B partial] route focused query workflows`), normal push succeeded; clean tree confirmed. This is not complete M1-B acceptance. Fresh named-role Diagram implementer assigned M2-A only; previous writer stopped. Diagram presentation has no dependency on Root-hint startup. No budget expansion or retry of that failure authorized.

## M2-A verification

Only `src/gui/diagram_studio.rs` changed: Source mode retains the prior Visual/Split preview mode, Show preview restores it, visible local zoom/Fit/100% controls retain existing Reset behavior, and both allocations use stable-ID draggable finite/clamped split ratios with narrow stacking. Presentation actions preserve source/dirty/history/selection and scene-build counts in actual UI tests.

Initial focused selection: 6/8 passed; constrained-width overflow and inner divider drag failed. First sizing correction: 7/8. Pointer-relative divider correction: exact drag test 1/1, then final assigned selection 8/8 (7,035 skipped). No known M1-B query test or broad suite ran. Four new presentation tests plus four existing layout/keyboard scope tests pass. Rustfmt touched only this file; parent diff/whitespace checks pass and no unrelated paths changed. Parent caught and requested restoration of an existing targeted lint attribute displaced by helper insertion. Native normal/constrained/DPI observations remain unrun until M6.

Exact final command: `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(studio_usability_diagram_presentation) | test(diagram_studio_default_split_keeps_canvas_visible_beside_inspector) | test(diagram_studio_minimum_width_stacks_split_and_keeps_visual_controls_reachable) | test(diagram_studio_canvas_keyboard_pan_zoom_and_arrow_nudges_are_scoped) | test(diagram_studio_source_focus_does_not_pan_or_zoom_canvas)'` — 8/8, Nextest 0.325s, cached Cargo profile 0.99s. Afterward only existing attribute placement and identical `180.0`→constant reuse changed; no numeric or behavioral change and no redundant rerun.

M2-A commit `46fba31e60446490ed210373d187544fc3b1a263` pushed normally; clean tree verified before assigning M2-B. M2-B handoff enforces pinned minimal offline library/raster stack, actual Windows three-family proof before async breadth, bounded source/structure/SVG/raster admission, one worker/latest queue and honest retirement, exact identity/revision/options, and no speculative publication. No template/viewer acceptance before required rendering works.

M2-B release audit (not render proof): implementer inspected fetched `mermaid-rs-renderer` 0.3.1 (MIT, edition 2024, default features disabled) and `resvg` 0.47.0 (Apache-2.0/MIT, intended features text/system-fonts only). Normal Cargo network failed once; one authorized exact-crate Cargo info fetch succeeded for each. Strict parser→graph→layout→SVG API and family structures present; no parse warning channel. No source/dependency edits at audit handoff. Actual lockfile resolution, offline resource guard and Windows rendering remain unverified.

M2-B proof progress: manifest uses exact `=0.3.1` and `=0.47.0`; lock resolution added 31 packages including required shared regex update (final resolved versions to record). First offline attempt stopped for uncached fontdb; authorized focused network run fetched dependencies and compiled pinned libraries and Windows app lib test target in 5m01s. Initial filter matched zero, not accepted; cached Nextest list (0.88s) confirmed actual names. Corrected execution: three-family production adapter test passed in 1.725s; bounds test failed only because it incorrectly treated exactly 4096×4096 (16,777,216 pixels) as over-limit. Fixture correction pending; worker/cache/Studio acceptance still pending.

Parent visually inspected `target/studio-launcher-usability/mermaid-{flowchart,mindmap,sequence}.png`: all three have readable labels and connections. This is library/raster proof, not native Studio UI acceptance. Test outputs have transparent padding beyond SVG intrinsic content; later preview fitting should use intrinsic bounds. Alpha format must remain explicit/consistent across raster, egui and PNG export.

Corrected proof command PASS: `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(diagram::rendering::tests::production_adapter_renders_flowchart_mindmap_and_sequence_to_rgba) | test(diagram::rendering::tests::production_adapter_rejects_invalid_oversized_nested_and_huge_raster_inputs)'` — 2 passed, 7,043 skipped, tests 1.031s/build 1m24s. Exact cap accepted, u32::MAX dimensions rejected. SVG external image resolvers return None; raster explicitly converts premultiplied tiny-skia pixels to documented straight-alpha RGBA. Adapter retains natural SVG dimensions. Worker/coalescing/cache/accepted Studio hook now being implemented; M2-B remains in progress.

Release limitation found in source: strict entry point can still ignore unknown flowchart lines. Bounded admission plan: reuse native parse_mermaid for qualified flowchart subset; mindmap noncomment lines must match parsed node accounting and reject directives/frontmatter; sequence accepts explicit participant declarations/message statements with matching parsed counts. Other kinds UnqualifiedFamily until C qualification. No universal second parser, template-exact whitelist, library patch or full-parity claim authorized.

Worker/cache progress: after quoting flowchart fixture labels to match documented native admission, five focused tests passed (1.173s tests/55.58s compile): real renders, rejected invalid/partial source, same-key/raster-only counters, replaceable latest queue, cooperative active retirement and stale document/revision/theme/raster rejection. Initial Studio hook needed one pub(crate) method visibility correction; subsequent six focused tests passed (1.066s tests/1m55s compile, 7,043 skipped).

Parent integration review withheld B acceptance pending ownership correction: replace RendererService::global with Studio-owned lazy service retained across document close/reopen; prevent global completion stealing and unnecessary global domain state. Do not run generic renderer in parallel with native-current flowcharts. Accepted-pass seam must use generic Mindmap/Sequence and correct root/child host wake in embedded fallback. Implementer fixing narrowly; M2-B remains in progress, no template/viewer acceptance.

M2-B final ownership correction complete: Studio-owned lazy `Option<RendererService>` retained across close/reopen; accepted generic-source hook works with native or embedded ROOT host, uses actual wake target, and leaves native flowcharts on native path. Searches found no global service references; one guarded spawn, cooperative retirement/shutdown without join or replacement. All required B acceptance now has focused evidence; template/viewer/native smoke remains later.

Six-test command PASS: `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(diagram::rendering::tests::production_adapter_renders_flowchart_mindmap_and_sequence_to_rgba) | test(diagram::rendering::tests::production_adapter_rejects_invalid_oversized_nested_and_huge_raster_inputs) | test(diagram::rendering::tests::cache_skips_parse_layout_for_identical_and_raster_only_requests) | test(diagram::rendering::tests::worker_admission_replaces_pending_and_retires_active_cooperatively) | test(diagram::rendering::tests::publication_rejects_stale_revision_document_and_options) | test(gui::diagram_studio::tests::diagram_studio_submits_generic_preview_only_after_retained_source_acceptance)'` — 6 passed, 7,043 skipped, compile 1m55s/tests 1.066s.

Final ownership command PASS: `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E 'test(diagram::rendering::tests::generic_preview_candidates_exclude_native_flowcharts) | test(diagram::rendering::tests::publication_rejects_stale_revision_document_and_options) | test(diagram::rendering::tests::worker_admission_replaces_pending_and_retires_active_cooperatively) | test(diagram::rendering::tests::cache_skips_parse_layout_for_identical_and_raster_only_requests) | test(gui::diagram_studio::tests::diagram_studio_submits_generic_preview_only_after_retained_source_acceptance)'` — 5 passed, 7,045 skipped, compile 3m12s/tests 0.958s. No known hint failure or broad suite selected. Diff/whitespace check passed.

Resolved material versions/features: exact mermaid-rs-renderer 0.3.1 (defaults off); exact resvg/usvg 0.47.0 (resvg text/system-fonts only, defaults off); tiny-skia/path 0.12.0, fontdb 0.23.0, rustybuzz 0.20.1. Existing tiny-skia 0.11.4 remains elsewhere. Shared regex advanced 1.11.2→1.13.1 to resolve renderer requirements; this actual shared dependency change belongs in final integration review rather than a claim of unrelated plugin requalification. `cargo tree --locked --offline -p multi_launcher -e features -i resvg` and corresponding `-i mermaid-rs-renderer` confirmed features. Cache limits: four SVG artifacts, three variants/artifact, total cached raster pixels capped at 16,777,216. No vendor edits/browser/temp interactive preview path.
