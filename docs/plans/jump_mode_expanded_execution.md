# Jump Mode expanded coverage execution

Authority: `multi_launcher_jump_mode_full_coverage_plan.md` and current `AGENTS.md`.
Started 2026-10-20 from clean branch `jump`, HEAD `0cb3dabf148dc720ed560626c32ef596b5e8ff9a`; retain selected branch and existing architecture.

## Checkpoints

| Milestone | Status | Evidence / commits |
|---|---|---|
| M0-A | complete | Read active plan, repository instructions and configured roles; inspected Git, owner bridge, renderer, dashboard gaps and isolated runner. |
| M1 | complete | M1-A `3c3b8e91` / M1-B `8648b2d0` / M1-C `a1181df6` pushed; 32 owner and 5 adapter tests passed, independent owner review clean; fixture-only PASS. |
| M2 File Search | complete | M2-A `b91740f3`, M2-B `5ffdf9a5`, M2-C `251f27e5` pushed; fixture PASS and eight focused QA/lifecycle tests passed. Optional native proof `not_run_environment`: repeated process-inventory denial before UI workflow. |
| M3 Snippets | complete | M3-A `f4ad295e`, M3-B `788cf705` pushed; M3-C `c5b32aef` pushed; lifecycle checks passed across one slice and exact repaired reruns. |
| M4 Clipboard History | complete | M4-A `84a78537` / M4-B `68b9838b` pushed; M4-C lifecycle and foreign-owner tests verified; grouped production review has no M4 findings. |
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

Snippets (M3-A): visible filter/Clear Filters/tag filters, snapshot-bound saved-row selection/Edit/Remove, Add/Close, alias/privacy/prompt toggles, tag input/Add/existing choices/removal chips, revealed multiline body, Save/Cancel/enabled Test/Preview, Reveal to Edit, and revealed prompt-field label/default/required/input-kind controls with scoped combo options. Concealed body/default controls and passive text are excluded. Pending inline removal and separate prompt own no parent targets; their explicit navigation is M3-B.

Snippets (M3-B): exclusive inline Confirm/Cancel bound to parent/confirmation instances; exact session-generation prompt fields and enabled Copy/Cancel or Preview Return to Editor. Parent targets, passive rendered preview and sequence-preflight Jump targets remain excluded. Sequence preflight retains ordinary owned keyboard handling.

Clipboard History (M4-A): visible opaque row selection, Edit/Remove/Close real responses. Generic labels exclude clipboard payloads. Passive heading and clipped rows are excluded; context-menu contents retain native access without Jump targets. Editor/Copy and exclusive removal are M4-B.

Clipboard History (M4-B): multiline editor/Save/Cancel, explicit Copy, and exclusive Confirm/Cancel removal. Parent row/Close targets are absent during confirmation; stale Confirm is disabled while Cancel remains available. System clipboard writes require later explicit Copy activation.

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
- M3-B routing dependency: `begin_root_frame_logic` currently dispatches prompt Escape/Ctrl+Enter/Tab before Jump. The exact typed prompt owner must run existing Jump capture first, retaining whole-frame `snippet_prompt_owns_input` and opening-frame key consumption. Prompt layer is `("snippet_prompt_window", session.generation)`; Preview exposes Return to Editor, never Copy; sequence preflight stays precisely blocked. Inline removal uses existing captured snapshot and exclusive child scope, not a new Window.
- M3-C verification should include a narrow noncommitting-pass draft assertion and ordinary default-off window activation/typing from another visible window. These directly exercise newly added native ownership gates; establish behavior with real egui frames before treating a suspected focus difference as a defect.
- M4 persistence dependency: existing `plugins::clipboard::update_history_file` owns transaction locking and atomic replacement. Add only a narrow checked snapshot operation there; preserve existing unrelated APIs. Use existing `ClipboardBackend` / fake backend writes for Copy tests, never the indexed reload command or the real clipboard. Preserve open edit drafts across clipboard revision changes.
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

Exact M2-C Rust command: `cargo nextest run --lib --features dev-smoke-binaries,qa-framebuffer-capture -E 'test(gui::qa_framebuffer::tests::phase2_fixture_capability_requires_exact_surface_identity_and_safe_allowlist) | test(gui::qa_framebuffer::tests::file_search_dialog_observation_reports_only_redacted_lifecycle_facts) | test(gui::qa_framebuffer::tests::phase2_request_is_root_f11_and_empty_context_is_paste_only) | test(gui::render::tests::jump_mode_file_search_lifecycle_reopen_rejects_stale_targets) | test(gui::render::tests::jump_mode_file_search_form_focus_types_then_enter_starts_one_search) | test(gui::render::tests::jump_mode_file_search_form_default_off_enter_still_starts_search) | test(gui::file_search_dialog::tests::escape_while_running_cancels_and_leaves_dialog_open) | test(gui::file_search_dialog::tests::escape_while_idle_closes_dialog)'`. Both native runner attempts used the default sandbox; no third attempt was made.

### M3-A evidence

`cargo nextest run --lib -E 'test(jump_mode_snippets_authoring) | test(body_editor_undo_state_is_isolated_between_edit_sessions) | test(hidden_metadata_only_edit_preserves_body_and_tags_without_reveal) | test(selected_tag_filters_match_case_insensitively_with_and_semantics)'`: seven selected; initially five passed and two failed on fixture visibility/preview eligibility. Exact repaired two-test selection then passed **2/2**, retaining the five earlier passes. No full-suite rerun. Scoped formatting and diff check passed. Incidental workspace formatting check reported unrelated baseline differences; those files were unchanged.

The actual renderer tests prove duplicate-row selection, then a distinct Edit hint and fresh Enter opening the expected second body; filtering/reload invalidation; concealed metadata absence, reveal, Unicode/multiline typing and Cancel; pending removal/prompt exclusion. Test clicks use actual clipped response centers; scroll assertions use the actual contents ScrollArea state rather than registering offscreen controls.

Snapshot generation and a separate view namespace protect row identity without persistence changes. Ordinary native input gating remains independent of Jump enabled state. Noncommitting passes use narrow shadow form values; native prompt-field widget salts retain their original session/key ownership. Parent inspected the source/render/test diff and requested these boundary fixes before commit. `preview_app` now disables Paste Next/Cancel before construction. No native session or real clipboard access in M3-A.

Repaired selection: `cargo nextest run --lib -E 'test(jump_mode_snippets_authoring_hidden_fields_stay_private_and_revealed_body_keeps_focus_until_cancel) | test(jump_mode_snippets_authoring_pending_removal_and_prompt_window_block_parent_targets)'` — **2 passed**. Final diff then added the same authorization guard to idempotent selection clearing and renamed a test-only ScrollArea output; scoped formatting/diff checks passed afterward, without another redundant test run. M3-C will exercise the noncommitting lifecycle boundary.

### M3-B evidence

`NavigationOwner::SnippetRemoval { parent_instance, confirmation_instance }` binds the actual Snippets layer to its exclusive captured-row Popup surface. Each removal request renews its confirmation/native button identities. Existing transition guards prevent hint+same-frame Enter or held repeat from confirming; a later distinct Enter removes only the generated fixture row through existing snapshot-checked persistence.

`NavigationOwner::SnippetPrompt { generation }` recognizes only the actual dynamic prompt layer. Existing Jump capture now precedes prompt shortcuts, retaining runtime and whole-frame prompt ownership. Native eligibility is independent of Jump enabled state. Foreign foreground, modal/popup/capture and stale-generation guards remain in force. A suspended Action Sheet is explicitly distinguished from rendered competing owners. Initial presentation may focus its actual first field under safe host/known-parent guards, while input/actions/hints remain disabled until exact layer ownership is established.

The prescribed seven-test selection plus one added real prompt interaction case passed across initial/focused repaired runs: **8 unique tests passed**. Evidence covers exclusive removal, held/same-frame key rejection, stale store rejection, generic/redacted metadata, prompt generation replacement, foreign-window Ctrl+Enter refusal, active-hint Escape priority, field hint/native typing, Copy hint with same-frame submit suppression, later injected fake copy once, Preview zero-copy semantics and sequence-preflight exclusion/native Tab. Generated stores and existing fake submission seams isolate all effects; fixture setup now installs an explicit temporary persistence override.

Initial compiler errors and invalid generated placeholder identifiers were corrected at their actual boundaries. The first-frame focus regression was fixed in presentation rather than weakening its test. Final `cargo check --lib --features qa-framebuffer-capture`, scoped formatting and diff check passed, including the equivalent removal Option-pattern cleanup. No full suite, Clippy or native session. Parent inspected the owner, routing, dialog and test diffs; grouped independent review remains scheduled for M2–M4.

Exact M3-B primary command: `cargo nextest run --lib -E 'test(jump_mode_snippets_privacy) | test(jump_mode_snippet_prompt) | test(filter_changes_and_new_targets_invalidate_pending_confirmation) | test(stale_removal_confirmation_keeps_external_update_unchanged) | test(snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog) | test(snippet_prompt_ctrl_enter_submits_once_and_preview_consumes_without_copy) | test(jump_mode_snippets_authoring_pending_removal_and_prompt_have_exact_owners)'`. Focused repaired selection used `test(snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog) | test(jump_mode_snippet_prompt_tracks_exact_generation_and_blocks_foreign_middle_owner) | test(jump_mode_snippet_prompt_escape_field_copy_hints_remain_focus_only_until_fresh_submit)`, then only `test(jump_mode_snippet_prompt_tracks_exact_generation_and_blocks_foreign_middle_owner)` for the final generated replacement-placeholder correction.

### M3-C evidence

Test-only integrated lifecycle coverage now proves duplicate-row Edit/Cancel/reopen/Save, exact unsaved Preview Return, noncommitting render-pass draft/store preservation, and default-off native Filter input after one real pointer selection from another foreground Window. All fixtures use generated stores; app test construction disables Paste Next/Cancel before construction. No production changes or native launches.

`cargo nextest run --lib -E 'test(jump_mode_snippets_lifecycle) | test(prompted_editor_cancel_discards_alias_and_text_draft_without_writing) | test(tag_editor_normalizes_drafts_and_preserves_save_cancel_reopen_state) | test(confirm_removes_the_selected_duplicate_row_only) | test(preview_cannot_resolve_or_copy_and_cancel_returns_exact_draft)'`: initially **7 passed, 1 failed**. The failed duplicate lifecycle was repaired only in fixture staging: scroll the actual contents area to clipped controls, use real mouse responses without unnecessary leader toggles, and establish a current registry after the reopened owner-transition frame. Exact duplicate lifecycle rerun passed. The exact default-off Filter case also passed after adding a pre-click `layer_id_at` assertion proving the actual field remains exposed. Eight unique tests passed; no redundant union rerun. Scoped formatting and diff checks passed. Parent inspected the cumulative test diff.

Native Snippets evidence remains `not_run_environment` under the previously exhausted process-inventory diagnostic/retry. No clipboard writes or production-data mutations occurred.
### M4-A evidence

Clipboard History now registers clipped-visible selection/Edit/Remove/Close responses with generic labels and opaque per-snapshot tokens, including distinct identical entries. Native row controls share token-based egui identity. Exact foreground/native eligibility stays independent of Jump enabled state; unowned responses disable and surrender only their own focus. Closed dialogs do not reload history on version notifications. Browse insertion/replacement renews mappings; updates during editing retain the draft and invalidate stale authority until safe refresh.

`cargo nextest run --lib -E 'test(jump_mode_clipboard_browse) | test(ensure_open_loads_saved_history_once_and_preserves_edit_draft)'`: **4 passed**. The extended insertion/replacement/draft test then passed its exact rerun, **1/1**. Initial compile feedback was two direct test probes using the old editor signature; their normal-owner argument was corrected. Scoped rustfmt and diff checks passed. Parent inspected source, renderer and tests. No native session. Existing save-failure draft handling is deliberately addressed by M4-B checked persistence, alongside explicit Copy and removal confirmation; M4-A does not claim those actions complete.
### M4-B evidence

Clipboard History now exposes editor Text/Save/Cancel and explicit row Copy, preserving the existing native multiline editor identity with edit-session isolation. Copy validates the full captured history and exact token/entry before using the existing ClipboardBackend; production uses Arboard, tests inject the existing fake and fail closed if it is missing. Clipboard I/O occurs outside the history transaction lock. Completion only focuses; later fresh Enter performs the existing action.

New ClipboardRemoval parent/confirmation instances bind exclusive inline Confirm/Cancel to the actual Clipboard History layer. All row/context-menu removals share the captured full-snapshot request, parent targets are withheld, distinct native confirmation identities reject carried keys, and stale confirmation remains visible with Cancel until dismissed. A same-version observer reload bug found during parent diff review was fixed before testing.

Checked Save/Remove compare the full expected snapshot within the existing update_history_file transaction lock and atomic replacement. Conflicts preserve external bytes and the draft/confirmation with payload-free feedback. Save exits and clears the draft only after success; I/O failure preserves it. Existing unrelated APIs and string-list serialization remain intact.

`cargo nextest run --lib -E 'test(jump_mode_clipboard_actions) | test(jump_mode_clipboard_checked_persistence) | test(long_history_entry_keeps_browse_and_edit_footers_reachable_without_mutating_text)'`: **5 passed, 1 failed**, then the sole corrected action test passed its exact selector. Six unique tests have passing evidence. Compiler feedback corrected nested Option scope construction. Fixture repairs corrected a reversed native response-ID assertion and established selection in the actual TextEdit state before real multiline Text input; no Ctrl+A claim or production keyboard change. Checked persistence, conflict/I/O draft retention, stale confirmation, held/same-frame key refusal, fake Copy once, fresh Save/Cancel and long-entry geometry passed. No native session or system clipboard mutation.
Final M4-B `cargo check --lib --features qa-framebuffer-capture`, scoped rustfmt and diff check passed. Parent inspected checked storage, owner routing, state/effect commitment and real-egui tests. The only later source change marked the obsolete test geometry wrapper `#[cfg(test)]`; no redundant verification run followed that annotation.
Grouped M2–M4 review is in progress against committed production through `68b9838b`, while M4-C adds only lifecycle tests. Preliminary substantive finding: File Search native ownership rejects every egui popup, causing existing Columns menu preference edits to be restored/suppressed. This compatibility regression remains open for a bounded sole-writer correction after the current test checkpoint; unsupported popup Jump targets must stay blocked.
Grouped independent review through `68b9838b` completed: one P2 File Search native-popup authorization regression; no additional substantive M3/M4 findings. Existing Columns preference changes roll back, result Calculate hash commands are dropped, and nested searches restore old form roots/mode after launch. Corrective scope is exact owned native popup provenance from real Columns/result anchors (Popup::default_response_id), using Export as a nearby pattern. Keep Jump blocked for these unsupported menus and reject foreign popup ownership. Source-only reviewer; no broad builds/tests. Correction remains open.
### M4-C evidence

Two test-only real-egui cases prove Clipboard close/reopen renews instance/target identity and excludes old mappings, while a fresh hint focuses the new Edit control without entering edit until later Enter; a foreign foreground Window blocks text input and targets without changing the private draft/store, then a fresh hint regains the actual editor after that Window closes. A/B evidence already covers clipping, default-off selection, multiline input, fake Copy and checked persistence, so it was not duplicated.

`cargo nextest run --lib -E 'test(jump_mode_clipboard_lifecycle) | test(jump_mode_clipboard_privacy)'`: **1 passed, 1 failed** at a test lookup before the post-reopen registry collection frame. After explicitly separating owner observation from stable collection, the exact `jump_mode_clipboard_lifecycle_close_reopen_rejects_old_target` selector passed. The final Edit assertion checks actual edit text rather than an unrelated browse-selection state. One malformed exact filter was rejected before test execution and corrected. Both unique tests passed across scoped runs; scoped rustfmt check/diff check passed. Parent inspected the entire two-test diff. No production changes, native session, clipboard access or harness work. Native evidence remains `not_run_environment` with existing diagnostic/retry receipts.