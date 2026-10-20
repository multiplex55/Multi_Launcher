# Jump Mode expanded coverage execution

Authority: `multi_launcher_jump_mode_full_coverage_plan.md` and current `AGENTS.md`.
Started 2026-10-20 from clean branch `jump`, HEAD `0cb3dabf148dc720ed560626c32ef596b5e8ff9a`; retain selected branch and existing architecture.

## Checkpoints

| Milestone | Status | Evidence / commits |
|---|---|---|
| M0-A | complete | Read active plan, repository instructions and configured roles; inspected Git, owner bridge, renderer, dashboard gaps and isolated runner. |
| M1 | complete | M1-A `3c3b8e91` / M1-B `8648b2d0` / M1-C `a1181df6` pushed; 32 owner and 5 adapter tests passed, independent owner review clean; fixture-only PASS. |
| M2 File Search | complete | M2-A `b91740f3` and M2-B `5ffdf9a5` pushed; M2-C fixture PASS and eight focused QA/lifecycle tests passed. Optional native proof `not_run_environment`: repeated process-inventory denial before UI workflow. |
| M3 Snippets | pending | |
| M4 Clipboard History | pending | |
| M5 Dashboard gaps | pending | |
| M6 JSON / Regex / XML | pending | |
| M7 Mouse settings / gestures | pending | |
| M8 Rename / Replace | pending | |
| M9 Designers | pending | |
| M10 Integration / evidence | pending | |
| M11 Final gates | pending | |

## Initial ownership and constraints

- `JumpModeUi` / `JumpModeScope` own frame registry, focus handoff and input guards. Existing owners: Root, Settings, Notes, Todo, Timer, PasteSequence, MkMacro, Blocked. Actual visible top Middle layer and focused layer determine eligibility; unknown layers fail closed.
- `src/gui/render.rs` assembles `NavigationWindows`, tracks lifecycles and renders owned scopes. New families currently lack typed owner routing; exact window IDs and instance lifecycle are M1 prerequisites.
- Dashboard slot/header/body namespaces already exist. Todo, Scratchpad and Calendar have partial coverage; Pinned Commands and Command History lack row registration.
- Preserve default-off setting, real response geometry/clipping, combo provenance, distinct later activation and same-frame input discard. No second engine/hooks, synthesized canvas drags or hidden target enumeration.
- Clipboard, concealed snippets, source bodies and match previews must stay out of IDs, badges and evidence. Mutation confirmations must own foreground input separately.
- Existing fixture generator disables Paste Sequence Next/Cancel, disables clipboard observers and validates empty typed clipboard/snippet stores; runner uses isolated profile/evidence roots and owned-process receipts.

## Verification budget and evidence

One focused Nextest slice per checkpoint, at most one adjacent owner slice where justified. No full suite or Clippy during M0–M9. M10 at most two integration slices. M11 one full workspace Nextest (at most one repaired rerun), ordinary checks/docs/format/commits then final Clippy.

Native sessions: 2 / 12; images: 0 / 24. File Search's two owned launches failed before workflow interaction because host process inventory returned Access denied. Both PIDs were absent after retained-handle cleanup; runner receipts retain `cleanup_uncertain`. One diagnostic and one retry exhausted for this environment failure. Do not repeatedly retry the same restriction for later surfaces; continue source/egui evidence unless the environment changes.

## Coverage inventory

Remaining checkpoint inventories are pending; individual unsupported pointer-only, passive or foreign-popup controls will be recorded with their actual boundary. No universal coverage claim.

File Search (M2-A/B): query, directory roots/Add folder/Remove, refinement/Clear, Filename/Content, Global/Directory, case/hidden toggles, Search/Cancel/enabled Open, Export anchor, matching/sort/type/extension/exclusion filters, Global roots/Filters/Diagnostics headings, enabled Copy diagnostics, inline Locate ripgrep/Dismiss; currently visible Filename rows (one eligible cell per row) and Content match responses. Only currently drawn enabled responses register. Generic Export menu options use native menu access and remain outside hints; native picker internals, passive group headings and truncation notices are excluded.

## Git and final gates

Parent owns commits and one best-effort normal push per checkpoint. M0-A `520bd40c` pushed successfully. Initial sandbox Git writes/network were unavailable; approved escalated Git commands succeeded. Full Nextest and final Clippy not run.

### M1-A evidence

`cargo nextest run --lib -E 'test(gui::jump_mode::ui::tests::) | test(actual_root_navigation_owner_follows) | test(opening_settings_on_a_discarded_root_pass)'`: **32 passed, 5,867 skipped**, exit 0. Scoped rustfmt and `git diff --check` succeeded. Configured reviewer inspected exact production IDs, lifecycle and frame guards: no substantive findings.

`NavigationDialogKind` / `NavigationOwner::Dialog` centralize identities; `JumpModeUi` observes per-open counters and `NavigationWindows` carries live instances. QA serialization includes the new typed owners without private state. No controls are claimed as covered yet.

Native Regex/XML immediate viewports and deferred Radial CentralPanel remain blocked in M1. M6/M9 must integrate their actual viewport context, registry revalidation and popup guards; this is a mandatory implementation dependency, not a permanent family exclusion. Ordinary root layers and embedded utility fallback windows are recognized exactly. Unknown foreground windows remain blocked.

### Bounded planner findings for upcoming checkpoints

- M1-B: extract existing optional registration wrappers from Notes/Todo/MkMacro into the shared bridge; preserve Select ownership and real native focus. M1-C: existing safe fixtures suffice; run `-FixturesOnly`, avoid claiming new native workflows through metadata alone.
- M2: existing `FileSearchResultRowId` supplies stable search/result/match/path identity. Add a view revision for filtering/sorting. Missing-ripgrep notice is inline; Export contains Copy actions in a generic menu, which stays blocked while native keyboard menu access remains. Real Enter dispatch needs exact-once egui proof.
- M3/M4: neither persisted model has row IDs; use opaque snapshot tokens without changing serialization. Snippets preview is currently hover-only; inline removal confirmation already exists. Clipboard lacks selection/Copy/removal confirmation, so those small explicit additions are required; validate captured snapshot before later action and never reload by saved index.
- M5: reuse slot scopes and generation. Actual gaps are Pinned/History response registration, Scratchpad multiline editor, Todo summary selectors/priority/done, Calendar month/date/event controls. Already-covered Copy/timestamp/Refresh/Add/Open controls must not be duplicated.
- M6: JSON has document generation and typed tree nodes. M6-B must bind existing-engine instances to exact viewports and consistently migrate scope registration, registry revalidation/combo provenance, prepare/complete and focus-intent checks; ROOT defaults remain unchanged. Native Regex owns its callback lifecycle, XML reuses it, and deferred Radial later reuses the same mechanism. No app-wide viewport map or duplicate hint algorithm.
- M7: session switches apply immediately, appearance remains draft, Mouse Gesture Settings is a separate actual window. Bind Action needs stable explicit identity. Use opaque session tokens for gesture/binding rows; exclude active capture/drag, unknown color popups and OS window picker. Preserve live-apply versus Save semantics.
- M8: use RuleDraft IDs, SnapshotEntryRef, scan/session revisions, SearchGeneration/ReplacementRevision/MatchId. Rename rows have real Include/Select checkboxes, not selectable labels. Replace inspection and approval are separate. Child IDs: default Confirm batch rename/Confirm Undo; explicit replace_apply_confirmation/replace_recovery_confirmation. Separate pass eligibility from effect commitment; backend commands and write authority remain untouched.
- M9: Dashboard editor has structural Select/header controls and hover-only drag preview; persistent IDs need duplicate/missing fallback. Radial uses deferred callback under existing editor lock; authored MenuId/RingId/CellId and dirty guards remain authoritative. Action editor is inline, not another window. Actual nested IDs and property renderers must be scoped explicitly; no coverage claim through an opaque widget registry call. Canvases remain pointer-only.

### M1-B evidence

Shared `register_and_apply_target` now owns optional-scope registration, order advancement and Focus application. Notes/Todo/MkMacro wrappers delegate; Select remains renderer-owned. Exact File Search Window bridge regression and migrated Action/Select test prove focus-only handoff.

Final focused slice: **5 passed, 5,895 skipped**; scoped rustfmt and diff check passed. Initial new-test rejection was the expected disabled first Window sizing pass; warming the actual Window fixed the fixture, and both the exact rerun and final slice passed. Parent inspected the entire adapter diff; no dispatch or target-kind changes.

Exact M1-B filter selected these `gui::jump_mode::ui::tests` names: `expanded_dialog_scope_bridge_focuses_only_the_real_current_response`, `a_real_response_receives_focus_once_without_clicking_or_activating`, `action_targets_request_logical_selection_without_running_actions`, `registry_excludes_disabled_clipped_zero_area_blocked_and_wrong_viewport_targets`, `owner_transition_blocks_all_discarded_passes_then_stabilizes` via `cargo nextest run --lib -E` test union.

### M1-C evidence

`pwsh -NoProfile -File dev/jump_mode/run.ps1 -FixturesOnly`: **PASS**, exit 0; generator tests **7 passed**, generation and typed verification exit 0. Receipt: `dev/jump_mode/evidence/jump_m01_baseline_20261020T211353Z_ec0d5cef/summary.md`. Executable is null; cleanup says not launched. Native budget remains 0/12 sessions, 0/24 images.

No fixture source changes were needed. `root_list_app_at` already constructs an inert plugin manager and disposable typed stores, with Paste Next/Cancel disabled before app construction. Existing disabled-bridge, unknown foreground, discarded-pass and combo-owner regressions cover readiness; M1-A's 32-test slice includes the core cases. Do not add redundant infrastructure.

### M2-A evidence

`cargo nextest run --lib -E 'test(gui::render::tests::jump_mode_file_search_form_) | test(gui::file_search_dialog::tests::escape_ui_consumes_key_after_file_search_handles_it) | test(gui::file_search_dialog::tests::switching_filename_content_mode_clears_selection)'`: final **6 passed, 0 failed**. Scoped rustfmt and `git diff --check` passed. Parent reviewed the form/filter/input/renderer diff.

Real renderer regressions prove hint focus followed by typing and one fresh Enter, default-off ordinary Enter, mode-dependent/disabled inventory and background field/button rejection. Exact query/root/refinement widget IDs now match keyboard helpers; the early handler consumes Enter once. Normal native eligibility is independent of the Jump enabled gate; the shared Jump resolver remains unchanged.

Intermediate failures were corrected at their owners: field-ID mismatch, a fixture forcing all popups visible, and ordinary-input gating when Jump is disabled. Diagnostics is tested while actually visible rather than bypassing clip checks. No result targets, backend changes, native sessions or screenshots in M2-A.

### M2-B evidence

`cargo nextest run --lib -E 'test(gui::render::tests::jump_mode_file_search_results_) | test(gui::file_search_dialog::tests::stable_keys_remain_unchanged_after_sorting_rows) | test(gui::file_search_dialog::tests::keyboard_activation_uses_selected_row_key_after_reordering) | test(gui::file_search_dialog::tests::hidden_rows_are_excluded_from_keyboard_navigation)'`: **7 passed, 5,901 skipped**. Parent then found a later-column native-focus mismatch; the narrow fix prefers an actually focused/clicked visible cell over the default first cell. Exact regression `clicking_non_first_filename_cell_preserves_native_row_focus_owner`: **1 passed, 5,908 skipped**. Scoped rustfmt and diff check passed.

Result scope shares the existing collector and binds instance/search/view revision. Complete existing row identities produce opaque local IDs; only clipped-visible responses register. Selection copies response IDs, never Responses or callbacks. Full current-row validation remains at Select/Enter; per-frame focus classification uses IDs/revision rather than cloning the hidden result model. Settings initialization moved into `FileSearchDialogState::from_settings` to keep private adapter state encapsulated.

Evidence includes duplicate Unicode basenames, exact Content match selection then fresh Enter preview, default-off row keyboard behavior, clipping and sort/refinement/new-search invalidation. No native sessions, external file opening or copying in this checkpoint.

### M2-C evidence

File Search now has a canonical `file-search` Phase2 fixture/QA/runner contract using the exact `fs` → `file_search:open` production opener. The profile contains one deterministic harmless file under its sole canonical isolated search root; Everything, external tools and unsafe plugins remain disabled. QA exposes query character count, lifecycle/search identity and form generation, without query text, roots or results.

`pwsh -NoProfile -File dev/jump_mode/run.ps1 -Scenario Phase2 -Surface FileSearch -FixturesOnly`: **PASS**, seven fixture-generator tests passed; receipt `dev/jump_mode/evidence/jump_m13_phase2_file-search_20261020T223540Z_1434fce1/summary.md`. Focused QA/lifecycle Nextest: **8 passed, 5,943 filtered**. Scoped formatting and `git diff --check` passed. Parent inspected all six changed files.

The added real-renderer lifecycle regression covers mode/inline-rg controls, unknown foreground picker blocking, close/reopen instance renewal and stale-target rejection, then default-off empty registry. Existing targeted tests cover Enter/Escape behavior.

Optional native workflow was **not_run_environment**. Normal invocation and the single retry both failed at host process inventory with `Access denied`, before workflow interaction or capture. Receipts remain truthful FAIL records: `dev/jump_mode/evidence/jump_m13_phase2_file-search_20261020T225218Z_f11a5e7f/summary.md` (PID 19584) and `dev/jump_mode/evidence/jump_m13_phase2_file-search_20261020T225308Z_c2d414de/summary.md` (PID 38092). Both retained-handle cleanup attempts were followed by absence of the exact owned PID. No native focus/typing/cancellation claim is made; no images, submitted search, open, copy or file mutation occurred.
