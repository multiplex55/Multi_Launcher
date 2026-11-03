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
| M1-A | complete | Focused Nextest 6 passed; explicit ordinary-owner layers and retained Mouse focus, strict automatic Show focus preserved; parent diff/commit boundary next |
| M1-B | pending | Full query execution, dependent on accepted A |
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
