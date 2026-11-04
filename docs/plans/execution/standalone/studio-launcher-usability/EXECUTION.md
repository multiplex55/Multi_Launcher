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
| M2-A | pending | Diagram presentation |
| M2-B / M2-C / M2-D | pending | Offline renderer / templates / viewer |
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
