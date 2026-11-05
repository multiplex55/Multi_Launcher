# Desktop Productivity Expansion execution ledger

Authority: ../../../../CODEX_GOAL_DESKTOP_PRODUCTIVITY_EXPANSION.md and companion Kickstart; selected IDs 1–46, 61, 88, 89, 119. One checkout, sequential source writer and category branches. No intermediate integration.

## C01/M0-A bootstrap

- Status: baseline prepared; application work not started.
- Initial checkout: master, 3d290ed95bad3caebd6bbf5cce10c54880389c6f, upstream origin/master at same SHA; index/worktree clean, no merge/rebase state.
- parent_branch: fix/studio-usability-root-jump-recovery
- parent_tip_sha: 7c49f083474afcdc0eb57eb3e88bbd32561165a8
- category_branch: feat/utility-dock-popout
- Verified initial child HEAD equals parent_tip_sha. Baseline SHA will be recorded after commit, avoiding self-hash recursion.
- Verified predecessor EXECUTION.md, INTEGRATION_STATUS.md and final native-stop report at its exact tip. Source repair accepted with focused 9/9 and independent review; native verification remains environment-blocked. No native success inherited.
- master descendants only relocated documentation and added this approved contract; no src/Cargo/AGENTS/.codex difference. Goal/Kickstart carried verbatim from master so the exact-parent child retains its current contract.
- Normal sandbox could not create .git/index.lock; authorized sandbox escalation successfully created the required child, without resets/stashing/merging.
- No unrelated user changes staged or discarded. No worktrees, auxiliary checkout/clone or agent branches created.
- Initial push: pending baseline commit.

## Execution order

| Category | Checkpoints | Implementation | Automated | Native | Review | Delivery |
| --- | --- | --- | --- | --- | --- | --- |
| C01 | M0-A, M0-B, M1–M4 | bootstrap | not run | inherited capture timeout; unverified | pending | pending |
| C02 | M0-A, M5–M7 | not started | not run | not run | pending | not started |
| C03 | M0-A, M8–M11 | not started | not run | not run | pending | not started |
| C04 | M0-A, M12 | not started | not run | not run | pending | not started |
| C05 | M0-A, M13 | not started | not run | not run | pending | not started |
| C06 | M0-A, M14 | not started | not run | not run | pending | not started |
| C07 | M0-A, M15–M17 | not started | not run | not run | pending | not started |

Next: M0-B owner/host inventory, then planner packet for M1-A. Preserve inherited unchanged native capability blocker; no repeated capture probes absent changed evidence. Push failure is nonblocking.

## Bootstrap receipt and selected-feature routing

- C01/M0-A baseline_sha: bc3d6f8f7ef67886402ad90a327198ba5ca1f09f.
- `git rev-parse HEAD^` returned 7c49f083474afcdc0eb57eb3e88bbd32561165a8.
- `git push -u origin HEAD`: exit 0; origin/feat/utility-dock-popout created, upstream configured. Baseline checkout clean.
- Inherited ignored final-handoff.json corroborates accepted predecessor and unchanged capture timeout after one user-restart probe. No new native capability evidence; preserve pending gates rather than repeat unchanged probes.
- Baseline `git diff --cached --check` flagged four existing Markdown hard-break lines copied from the approved Goal; retained as intentional contract formatting. No source changes.
- Read-only planner dispatched using configured planner role; no independent implementation or Git ownership.

| Selected IDs | Owner/checkpoints | Current coverage |
| --- | --- | --- |
| 1–4, 7, 10 | calculation integer domain/grammar M5 | pending |
| 5–6, 8, 11 | separate inspector/comparison M6 | pending |
| 9, 12 | unit algebra/examples/reference M7 | pending |
| 13–15 | Notes/Markdown/MkMacro M2 | pending |
| 16–18 | Settings/Dashboard/mouse M3 | pending |
| 19–23 | remaining host families M4 | pending |
| 24–26 | shared session/geometry/focus M1 plus every migration | pending |
| 27–28, 34–35 | diagram authority/model M8 | pending |
| 29–33, 36–37, 39–41 | visual manipulation/connectors/assets M9 | pending |
| 38, 42–43, 45–46 | diagram assistance/export/library M10 | pending |
| 44 | draw.io subset M11 | pending |
| 61 | verified Explorer folder tabs M12 | pending |
| 88 | read-only folder sizes M13 | pending |
| 89 | read-only reported file users M14 | pending |
| 119 | silent GIF and MP4 M15–M16 | pending |
| All 50 | cumulative review/evidence/handoff M17 | pending |

No approved selection is implemented or accepted merely by appearing in this routing table. Detailed host rows follow the M0-B live-source inventory.

## M0-B bounded baseline inventory

Read-only configured planner validated all rows at bc3d6f8f. No builds/test execution/native probes were needed. All new migration/state checks are pending; all native checks remain unverified. Native capture blocker unchanged: prior supported screenshot path timed out even on Notepad, including final post-user-restart probe; do not repeat unchanged probes. Native flows N1–N12 are pending, not passed by this inventory.

Tools: `cargo nextest --version` succeeded (0.9.135); `rustc --version` succeeded (1.99.1). Cached fast-dev executable exists but predates recovery source; not valid native evidence of this branch. Existing fast-dev profile retained; no nextest/profile configuration changed.

Paths below are relative to src/gui unless otherwise stated. D=docked; N=native immediate with embedded fallback. Every row needs its own future wiring/state check even if native evidence samples families.

| Surface | Authoritative owner/current host | Close, worker and child ownership | Existing size key / focused test route |
| --- | --- | --- | --- |
| Quick Notes | notes_dialog.rs::NotesDialog / D | Existing note store; search/edit state; template manager/delete prompts | quick-notes / quick_notes_wrap_links_saves_closed_note_and_refreshes_entries |
| Each note editor | note_panel.rs::NotePanel / D, multiple instances | Note/baseline/history/selection, close/save/conflict and attachment/table children per panel | note-editor / studio_usability_note_close_prompt_can_save_and_conflict_prompts_are_panel_owned |
| Markdown Studio | markdown_studio.rs::MarkdownStudioDialog, MarkdownDocument / D | Accepted effects, dirty decisions, picker/cache, exact fence origins | key absent / markdown_studio_diagram_source_handoff_replaces_exact_fence_as_one_undo |
| MkMacro | mkmacro_dialog/mod.rs::MkMacroDialog / D | Draft/baseline/selection; runtime/recorder/overlay/pickers survive movement | mkmacro + child keys / existing authoring tests, new no-replay fixture needed |
| Main Settings | settings_editor/state.rs::SettingsEditor / D | Draft validation; root owns explicit save/apply; nested capture/settings | settings / paste_preferences_survive_unrelated_editor_save_and_validate_specified_chords |
| Dashboard Settings | dashboard_editor_dialog.rs::DashboardEditorDialog / D | Draft; accepted save/close; removal child prompt | dashboard-editor / failed_coalesced_save_and_close_keeps_editor_open_and_draft_retryable |
| Mouse Settings | mouse_settings_dialog.rs::MouseSettingsDialog / D | Draft/baseline CoordinateToolPreferences; explicit result/save/cancel | mouse-settings / effect_status_and_saved_summaries_are_truthful_and_use_committed_values |
| Mouse Gestures | mouse_gestures_dialog.rs::MgGesturesDialog / D | Draft/committed DB, GestureRecorder, binding/delete children | mouse-gestures + gesture-binding-editor / existing recorder tests + migration |
| Gesture Settings | mouse_gesture_settings_dialog.rs::MouseGestureSettingsDialog / D | Draft/reload and ignore-window picker | gesture-settings + picker / constrained_settings_scroll_keeps_footer_reachable_and_close_preserves_live_settings |
| File Search | file_search_dialog.rs::FileSearchDialogState / D | SearchBackend/SearchId; results/selection; preview/missing-tool prompts | file-search / selected_content_match_survives_group_reorder |
| Diff | diff_dialog/mod.rs::DiffDialogState / D | DiffWorkspace/text/binary/folder/watch; previews/dirty/input children | diff + child keys / memory_text_close_cancel_keeps_dirty_comparison_and_undo |
| Batch Rename | batch_rename_dialog.rs::BatchRenameDialogState / D | Ticket/generation/frozen plan; RenameScanCoordinator; apply/undo confirmation | batch-rename / m8d_confirmed_plan_is_frozen_and_draft_edits_revoke_apply_authority |
| Search & Replace | search_replace_dialog.rs::SearchReplaceDialogState / D | Search ticket/candidate; ReplacementPreviewWorker/receipt; explicit write/recovery | search-replace / m9d_selected_match_freezes_bounded_candidate_without_writing_source |
| Clipboard History | clipboard_dialog.rs::ClipboardDialog / D | Edit snapshot/checked persistence; root one-shot effects | clipboard-history / jump_mode_clipboard_checked_persistence_preserves_draft_on_conflict_and_io_failure |
| Snippets | snippet_dialog.rs::SnippetDialog / D | Edit/baseline/checked store and expansion prompt | snippets + snippet-prompt / external_append_body_and_privacy_changes_reject_stale_snapshot |
| Clipboard Modify | clipboard_modify_dialog.rs::ClipboardModifyDialogState / D | PreviewCoordinator/pipeline draft; external/large/save/delete confirmations | clipboard-modify / preview_can_be_cancelled, pipeline_deletion_and_reorder_save_immediately_and_preserve_drafts_on_failure |
| Paste Sequences | paste_sequence_dialog.rs::PasteSequenceDialog / D | Root playback/hotkeys; close_from_window ends session, never call for migration | paste-sequences / playback/second-run regressions + paste_sequence_confirmed_apply_failure_rolls_back_and_never_reuses_id |
| JSON | json_utility_dialog.rs::JsonUtilityDialogState/JsonEditorState / D | Parse worker/generation; close retires, open loads clipboard, neither is migration | json-utility / worker_discards_stale_results_and_coalesces_pending_source |
| XML | xml_toolkit_dialog.rs::XmlToolkitDialogState / N | SessionLifecycle/engine/file worker; overwrite generation; native Jump | xml-toolkit / m7f_xml_session_workflow_uses_one_current_session_for_real_parse_xpath_format_xsd_and_reopen |
| Regex | regex_tester_dialog.rs::RegexTesterDialogState/RegexSession / N | Evaluation/debounce; native Jump; accepted visualization effects | regex-tester / regex_tester_open_is_idempotent_and_preserves_session_draft |
| Table | table_utility_dialog.rs::TableUtilityDialogState/TableUtilitySession / D | Session parse/projection/paging; close retires and clears export; fenced overwrite | table-utility / table_dialog_overwrite_buttons_revalidate_the_captured_export |
| Timer/alarm authoring | timer_dialog.rs::TimerDialog / D | Draft/picker; scheduler outside host; completion transient separate | key absent / actual_duration_picker_edits_submit_exact_timer_once |
| Calendar root | calendar_popover.rs::CalendarPopover + LauncherApp date/open / D | Month/view/dashboard snapshot; separate child owners | calendar / new wiring/state test needed |
| Calendar event editor | calendar_event_editor.rs::CalendarEventEditor / D | Draft/recurrence validation/transactional save | calendar-event-editor / editor_commit_uses_latest_valid_event_and_rejects_corruption |
| Calendar event details | calendar_event_details.rs::CalendarEventDetails / D | Event instance; duplicate/delete transactions and confirmation | calendar-event-details / duplicate_and_delete_use_transaction_and_reject_corruption |
| Date & Time | date_time_studio_dialog.rs::DateTimeStudioDialog / D | Staged frame effects/local draft/close | key absent / date_time_studio_normal_window_controls_fit_and_copy_notice_preserves_focus |
| Image Compare | image_compare_dialog.rs::ImageCompareDialogState/Arc<ImageCompareSession> / D | Presentation worker/uploads/camera retired only on actual close | image-comparison / imgdiff_dialog_reentry_reuses_idle_session_and_retires_old_upload_budget |
| Duplicate Finder | duplicate_finder_dialog.rs::DuplicateFinderDialogState/DuplicateFinderSession / D | Scan/verified actions; close cancels/clears rows | duplicate-finder / dff_dialog_response_flow_cancels_drains_restarts_and_uses_current_verified_actions |
| ZIP Extractor | zip_extractor_dialog.rs::ZipExtractorDialog + archive::ArchiveSession / D | Session/worker outside host; extraction/overwrite/unsafe decisions explicit | zip-extractor / workflow_controls_stay_reachable_and_drive_preview_confirmation_and_extract |
| Calculation Pad | calculation_pad_dialog.rs::CalculationPadDialog/CalculationPadSession / D or Popout | Generation/accepted-pass fencing, recovery, dirty transition/chooser/save/examples; fresh reopen docked | key absent / studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision |
| Diagram Studio | diagram_studio.rs::DiagramStudioDialog/DiagramDocumentController / N | Owner generation, accepted mutation/intent, renderer, source/canvas, dirty/OS close | key absent / diagram_studio_child_discard_does_not_commit_visual_transaction |
| Diagram viewer | Same document owner, separate native camera/generation | Read-only current document; independent close/lifetime, no second editable session | key absent / studio_usability_diagram_viewer_discard_close_and_reopen_are_generation_scoped |
| New inspector/C04–C07 tools | Respective future domain/UI owner | Lazy domain/worker lifetimes independent of host | New bounded keys and dedicated tests in owning checkpoint |

M0-B result: all required surfaces and selected IDs mapped; architecture differs from broad notes wording in the explicit NotePanel and Calendar owners above. UtilityToolKey already bounds/preserves opaque data; M1-B adds absent keys/positions. render.rs already schedules Pad/Diagram/XML bootstrap when hidden. Existing Diagram provenance can reuse common primitives while keeping document/viewer authority distinct. No unresolved ancestry ambiguity or source blocker identified. Inventory complete; feature implementation/native acceptance still pending.

Next approved packet M1-A: add small gui/utility_host.rs owning typed presentation/generation/bootstrap/focus/retirement and retained frame-pass provenance; migrate Calculation Pad without copying/reopening session; reuse applicable Diagram owner/viewer provenance. Keep all domain effects and dirty decisions in existing owners, accepted source/history-before-intent ordering, hidden-root/embedded input protection. Geometry and other tools excluded. Add common transition/idempotence/stale/discarded-pass tests and retain exact existing Pad/Diagram regressions. Sole implementer runs one focused Nextest selection after nonzero-match confirmation; max two corrections for the same failure. Parent owns Git/review/ledger.
