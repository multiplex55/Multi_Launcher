# Productivity Toolkit Expansion

| Git identity | Value |
| --- | --- |
| parent_branch | feat/calc-pad-dynamic-vectors |
| parent_tip_sha | a9de4ab4be045bdadd27a910425d51dd1c6f0779 |
| Task branch | feat/productivity-toolkit-expansion |
| baseline_sha | 768fbe9a93268f21509c89fe55f54b96c909f8ad |
| Upstream | origin/feat/productivity-toolkit-expansion |

M0-A verified the direct parent and clean checkout. The intentional baseline adopted the two approved Toolkit documents from a89a1e6cd. Initial push succeeded with exit 0. The single existing checkout was used throughout.

| Checkpoint | State |
| --- | --- |
| M0-A | complete |
| M0-B | complete |
| M1-A | complete, with interactive smoke pending |
| M2-A | complete |
| M2-B | complete, with interactive smoke pending |
| M3-A | complete |
| M3-B | complete |
| M3-C | pending |
| M4-A, M4-B, M4-C | pending |
| M5-A, M5-B | pending |
| M6-A, M6-B | pending |
| M7-A, M7-B | pending |
| M8-A, M8-B | pending |
| M9-A, M9-B, M9-C, M9-D | pending |
| M10-A, M10-B | pending |
| M11-A, M11-B, M11-C, M11-D | pending |
| M12-A, M12-B | pending |
| M13-A | pending |

## M0-B platform decisions

- PDF uses a shared point-based page model with positioned text, graphics, links, and resolved local assets. Use a single Rust PDF writer with embedded Unicode font subsets and annotations (Krilla 0.8 is the selected candidate), and retain the existing Markdown parser and asset permission checks. Choose redistributable font assets and disclose missing glyphs. Render representative exports with Poppler and inspect page size, page count, extracted text and links.
- Print uses the Win32 PrintDlgExW printer context and GDI's StartDoc, StartPage, EndPage, EndDoc and AbortDoc sequence, driven by the same page model. Validate printable bounds and report scaling or hard-margin differences. Use Microsoft Print to PDF as a disposable submission target. The spooler is running, and the printer is visible when queried outside the sandbox. No print job was submitted during setup.
- External notifications use UserNotificationListener with the manifest's uap3:userNotificationListener capability. Probe the optional signed external-location identity route first with a single foreground consent call. The process has to hold the expected package identity, consent has to run on the UI thread, and access must be checked before and after toast snapshot reads. A registered package alone does not validate the consent host. If that host fails, the supported fallback is an optional UWP foreground listener with same-session, package-SID-scoped local IPC. Keep any third-party text off disk by default.
- Windows SDK 10.0.26100 MakeAppx and SignTool are installed. No matching MultiLauncher package was found by Windows PowerShell, and no existing current-user signing certificate was reported. Packaging/consent proof remains unverified until a trusted signed test package can be registered in an isolated test context. No trust or system policy was changed. Ordinary builds must stay independent of these tools.

Sources: [notification listener](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener), [manifest capability](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap3-capability-manual), [optional package identity](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps), [UWP/Win32 IPC](https://learn.microsoft.com/windows/uwp/communication/interprocess-communication), [printer context](https://learn.microsoft.com/en-us/windows/win32/printdocs/printer-output), [GDI print API](https://learn.microsoft.com/en-us/windows/win32/printdocs/gdi-print-api-functions), [Krilla API](https://docs.rs/krilla/0.8.2/krilla/).

## M0-B initial bounds

Use existing Pad/vector/archive and Markdown source and asset limits. New tools start with these caps and report saturation explicitly.

- PDF: 256 pages, 64 MiB serialized output and 64 MiB decoded assets per render, with checked dimensions and cancellation during layout.
- Transfer: 32 active or queued jobs, 10,000 entries per reviewed job, 128 directory levels, 32,768 UTF-16 units per Windows path and 32 MiB of receipt metadata. Keep unresolved recovery records when compacting completed history. Stream contents independently of metadata limits.
- Downloads: 10,000 direct children per explicit scan, 10,000 reviewed-state entries, 16 KiB per origin record. Surface truncated scans and read errors.
- Inbox: 7 days and 1,000 entries by default, settings bounded to 1–365 days and 100–10,000 items, including pins. Cap external snapshot payloads to 512 entries and 1 MiB, and cap retained title/body lengths. Publish terminal/attention events once and disclose truncation.
- Secrets: 8–256 password characters or 3–32 words, with at most 1,024 UTF-8 bytes per transient result.

## M0-B source reconciliation

- Pad: calculation_pad/document.rs and loops.rs both prepare/evaluate through script.rs. Extend the Function catalog and vector.rs CollectionOperation, including collection-failure rollback and the separate ordinary and vector fuel. Named callbacks must receive a distinct source role in rename.rs and syntax.rs, with real definition origins surfaced by DependencySnapshot. Examples & Reference retains CalculationExample; explicit insertion uses the existing source-edit transaction. ChartDefinition/ChartDataset/ChartScene in chart.rs, persisted view validation and Pad session rename/undo share the existing ChartRuntime/ChartActions and its current evaluation stamp.
- Markdown: parse_headings currently recognizes ATX headings. Section ranges need actual heading-end offsets for setext. NotePanel uses apply_prepared_markdown_edit_with_selection and Studio uses apply_prepared. Studio needs its first outline adapter. Shared parsing and export belong in notes_markdown with the existing Markdown export asset resolver and publication rules.
- ZIP: ZipExtractorDialog fences picker results by host, mode and draft revision. Advance destination through the real draft and ArchiveSession. The archive destination currently requires an existing held directory. Admit an absent exact child with a held parent for preview, then create it exclusively in the confirmed writer after validation. Preserve no-clobber and token fencing.
- Secrets: plugins/random.rs currently publishes a clipboard action string. Use typed ephemeral invocation ahead of gui/actions.rs record_history_usage and history.rs. ClipboardPlugin update_history and Clipboard Modify snapshots both need native same-clipboard-item exclusion.
- Files and inbox: file_operations.rs provides native_file_identity, guarded access/publication and cancellable scanning. File Search's SelectedFileSearchResult provides exact selections. Transfers own approvals and execution, publish monitored ActivityTask outcomes and issue terminal events to the inbox. Preserve timer completion identity at the timer owner, use calendar and Todo domain adapters, and integrate sanitized launcher toasts, the macro notification backend and Hot Folder terminal events.
- Keyboard M12-A: existing Jump registrations cover File Search, Batch Rename, Search/Replace, JSON/XML/Table, Duplicate Finder, Image Compare, ZIP, Clipboard History/Modify, Activity, Snippets, Regex, mouse forms, Hotkey Directory, Dashboard and Radial. Specialized owners include settings, timer, Todo, Paste Sequences and macro. Audit their remaining ordinary controls and add coverage to File Search Preview, Hash, QR, File Lock, Folder Size, Explorer Tabs, Date/Time, Convert, Hot Folder, Data & Recovery and Calendar.
- Keyboard M12-B: audit Notes/NotePanel and its Template, Unused Assets and Image Panel companions, Markdown Studio, Pad/Reference/Dependencies/Chart/Sensitivity, Programmer Inspector, Diagram, Plugin Editor and the newly introduced dialogs. Ordinary controls surrounding canvases are in scope. Native picker internals and canvas pan/draw/resize gestures use their existing input ownership.
- Source differences from the planning reference include minor ZIP and archive lint corrections. The exact parent has an inherited clean-build defect: drawio.rs has a test-time include of a missing archived fixture. Master already repairs it with a source-adjacent fixture. Adopt that narrow test fixture repair before library Nextest, independently of feature code.

No application build was run in M0-B. The Nextest binary is 0.9.135 and the active Rust toolchain is 1.99.1.

## M1-A ZIP destination

M0-B was pushed at 8ec6cbb23ce014163dc817616de0ba76f7a00e4d. The narrow test fixture repair was pushed at 4e7e6b101d18f4030dd4e143a9571ada6d656a4a.

The ZIP dialog suggests the adjacent stem for a newly selected archive, retains same-source manual overrides and exposes the exact reviewed layout. The session carries held directory authority through preview, and confirmation creates the initially vacant root exclusively relative to its held parent. A competing root is preserved. Terminal results account for root creation even if a later operation fails. Rejected proof is discarded so another explicit preview can check the current destination.

The first targeted library Nextest run passed 10 cases. The final scoped run passed 12 of 12 after review corrections, covering derived names, canceled and stale pickers, no write before confirmation, exact nested extraction, competitor survival and retry. Runner flags: cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1, with archive/session and ZIP GUI test filters. Each test had a 100-second cap after compilation. The initial sandboxed cargo check could not access the build lock; the elevated test build passed. Independent follow-up review closed all four ZIP findings. Staged diff whitespace check passed.

The Windows filesystem and retained dialog tests ran. Standalone application picker smoke remains part of the native evidence work. Disposable settings and ZIP fixtures are ready under target/productivity-toolkit-expansion-evidence/m1a.

The final M1-A Nextest expression was test(session_vacant_destination) | test(session_failed_vacant_preview_reopens_current_destination_on_retry) | test(session_cancelled_new_root_can_be_reviewed_and_retried) | test(suggested_destination_preserves_stem_and_rejects_invalid_names) | test(selected_archive_suggests_destination_and_preserves_same_source_override) | test(extract_picker_cancel_and_stale_results_preserve_drafts) | test(session_preview_keeps_held_handles_and_has_no_effect_before_confirmation) | test(session_destination_change_discards_late_preview_and_uses_current_root) | test(session_source_replacement_invalidates_completed_preview_token) | test(session_preserves_competitor_created_after_preview) | test(create_picker_rejects_cancelled_and_stale_owners_and_drafts). M1-A was pushed at 6940a16d46be786ce3855523ee7c68345361ee2f.

## M2-A ordering and uniqueness

The shared collection path handles sort, sort_desc, and unique. Stable numeric ordering treats signed zeros as equal. Uniqueness keeps the first occurrence, including its exact zero representation. Existing document-local formulas with any of the new names retain exact-spelling precedence. Prior built-in reservations and recursion rules remain in force. The Pad guide and executable catalog describe the new functions.

Both checks used cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1, with the following -E expressions.

- test(ordering_and_uniqueness_) | test(vector_sort_and_unique_work_in_both_evaluator_routes) | test(old_local_functions_can_shadow_new_vector_names) | test(vector_collection_operations_are_pure_and_compose) | test(collection_failure_preserves_current_binding_in_both_evaluator_routes) | test(catalogue_search_finds_function_keywords_and_vector_examples) | test(all_examples) passed 7 of 7. The last selector matched no tests.
- Cached test(shared_examples_evaluate_to_their_documented_results) passed 1 of 1.

Independent review found no blocking issues. Diff whitespace verification passed.

## M2-B named vector callbacks

M2-A was pushed at 2ccf975ca0079dc9ee01f6ade2c279167e385ce0. Map, filter, count_if and find_indices resolve a bare previously declared formula, validate callback shape even on empty input, and run through the existing formula invocation and work limits. Explicit extra arguments are evaluated once, and callback output publishes only after success. Invalid setup remains lazy in unselected document branches and preserves prior bindings on evaluated failures. Formula definitions with invalid callback setup are rejected. Function rename, undo, editor completion and dependencies follow the actual callable rather than a same-spelling value or physical row reference.

Focused Nextest passes were 6 of 6 initially, 13 of 13 after review corrections, and 14 of 14 on the final frozen source. The final build took 1m40s and test execution took 0.558s. The final command was cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 with -E:

test(vector_callback) | test(callback_records_nested_definitions_only_when_called) | test(invalid_callback_setup_preserves_prior_binding_in_both_routes) | test(prior_local_callback_operation_names_keep_function_precedence) | test(function_rename_tracks_calls_and_callbacks_without_capturing_values) | test(row_named_formula_is_distinct_from_physical_row_reference) | test(unresolved_callback_is_not_a_document_value_reference) | test(callback_identifier_completion_inserts_bare_function_name) | test(callback_function_rename_is_one_undoable_edit) | test(shared_examples_evaluate_to_their_documented_results) | test(old_local_functions_can_shadow_new_vector_names) | test(parameter_scope_capture_and_malformed_sources_are_checked)

The reviewed corrections and the invalid-draft callback role edge are covered. Diff whitespace verification passed.


## M3-A cumulative and rolling vectors

M2-B was pushed at eab988ea1de45f541e0b604b1290b8774ccc9871. The eight C3 functions return reusable vectors. Rolling outputs use complete trailing windows, and the chart fixture tested explicit X alignment. The review correction prevents an overflowing subtraction in the mean of opposite-sign extrema. Adjacent percentage changes use ratios and reject zero prior values.

Both runs used cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1:
- Initial -E test(cumulative_and_rolling_) | test(rolling_windows_bound_work_and_handle_large_values) | test(previously_declared_rolling_names_remain_local_functions) | test(chart_rolling_series_requires_explicit_aligned_x_values) | test(shared_examples_evaluate_to_their_documented_results) | test(vector_collection_operations_are_pure_and_compose) passed 7 of 7.
- Frozen-source rerun -E test(cumulative_and_rolling_vectors_follow_complete_window_semantics) | test(rolling_windows_bound_work_and_handle_large_values) passed 2 of 2.

Diff whitespace verification passed.


## M3-B quantiles and sample statistics

M3-A was pushed at 3c11de694918eafc0deb86f36b5b5128b101547e. Quantile and percentile use linear interpolation over a sorted copy. New dispersion calculations center and scale values ahead of moment calculation, and sample standard deviation is evaluated directly even when sample variance would overflow. The existing population stdev keeps its existing definition. The new functions return ordinary scalars and use the existing collection error and work boundaries.

cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E with test(univariate_statistics_) | test(previous_local_statistic_names_preserve_function_precedence) | test(shared_examples_evaluate_to_their_documented_results) | test(vector_collection_operations_are_pure_and_compose) passed 6 of 6 on frozen source. Independent review found no substantive issues. Diff whitespace verification passed.
