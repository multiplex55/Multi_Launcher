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
| M1-A | complete, with partial interactive smoke |
| M2-A | complete |
| M2-B | complete |
| M3-A | complete |
| M3-B | complete |
| M3-C | complete |
| M4-A | complete |
| M4-B1 | complete |
| M4-B2A | complete |
| M4-B2B | complete |
| M4-B3A | complete |
| M4-B3B | complete |
| M4-C1A | complete |
| M4-C1B | complete |
| M4-C1C | complete |
| M4-C2A1 | complete |
| M4-C2A2 | complete |
| M4-C2B1 | complete |
| M4-C2B2 | complete, with partial interactive smoke |
| M5-A1 | complete |
| M5-A | complete |
| M5-B1 | complete |
| M5-B | complete, with partial interactive smoke |
| M6-A | complete |
| M6-B | implementation complete, live docked moves and Undo verified in both editors, popped-out move unverified |
| M7-A1A | complete |
| M7-A1B1 | complete |
| M7-A1B2 | complete |
| M7-A1B3 | complete |
| M7-A2A | complete |
| M7-A2B | complete |
| M7-B1A | complete |
| M7-B1B1A | complete |
| M7-B1B1B | complete |
| M7-B1B2 | implementation complete, both live Save As and Cancel verified, live file acceptance unverified |
| M7-B2A | complete, real Windows virtual print verified |
| M7-B2B | implementation complete, live chooser and Cancel verified, live device acceptance unverified |
| M8-A1 | complete |
| M8-A2 | implementation complete, focused admission verified, live clipboard persistence unverified |
| M8-A3 | complete |
| M8-B | implementation complete, live generation/reveal/restart verified, live clipboard unavailable |
| M9-A1 | complete |
| M9-A2A | complete |
| M9-A2B | complete |
| M9-B1A | complete |
| M9-B1B | complete |
| M9-B2A | complete |
| M9-B2B | complete |
| M9-B3A | complete |
| M9-B3B | complete |
| M9-B3C | complete |
| M9-B3D1 | complete |
| M9-B3D2 | complete |
| M9-C1A | complete |
| M9-C1B1 | complete |
| M9-C1B2 | complete |
| M9-C1B3 | complete |
| M9-C2A | complete |
| M9-C2B | complete |
| M9-C2C1 | complete |
| M9-C2C2 | complete |
| M9-D1 | complete |
| M9-D2 | complete |
| M10-A | complete |
| M10-B | complete |
| M11-A | complete |
| M11-B | complete |
| M11-C | blocked |
| M11-D | blocked |
| M12-A1 | complete |
| M12-A2 | complete |
| M12-A3 | complete |
| M12-A4a | complete |
| M12-A4b | complete |
| M12-A5a | complete |
| M12-A5b | complete |
| M12-A6a | complete |
| M12-A6b1 | complete |
| M12-A6b2 | complete |
| M12-A | complete |
| M12-B1a1 | complete |
| M12-B1a2 | complete |
| M12-B1b | complete |
| M12-B2a1 | complete |
| M12-B | pending |
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


## M3-C paired statistics and fits

M3-B was pushed at 73facc2ce65dcaddde19f52540df3e4f6236eb12. Paired statistics use bounded centered/scaled moments. Compensated means and an exact fallback for lost or subnormal normalized cross terms preserve significant values for extreme data. Existing rational arithmetic combines factors before rounding an ordinary scalar output. Intercept and prediction retain precise raw means and are independent of rounded or overflowing public slope. Constant Y can yield valid slope, intercept and predictions while leaving correlation and R-squared undefined.

The initial and final frozen-source cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 invocations each passed 7 of 7. The -E expression was test(paired_statistics_) | test(univariate_statistics_) | test(shared_examples_evaluate_to_their_documented_results). The final run covered review fixes for a materially inaccurate subnormal covariance and lost intercept from extreme cancellation. Diff whitespace verification passed.


## M4-A histogram and empirical distribution data

M3-C was pushed at 785af3f491d612cec827fac592cc6c61d63c4185. Chart distribution data retains exact rational intervals, including a complete final requested-width bin even when its upper edge exceeds the finite scalar range. Count and automatic bins span the observed minimum and maximum. Each observation belongs to one bin. ECDF steps group duplicate observations and signed zero. Both accessible summaries and the future scene use the same counts and boundaries. UI integration remains M4-C.

cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E with test(distribution_) | test(chart_empty_adapter_keeps_missing_mismatch_and_hidden_states_distinct) passed 5 of 5 on frozen source. Independent review found no substantive issue. Staged diff whitespace verification passed.


## M4-B1 options model and compatibility

M4-A was pushed at 2b4e3398525ea0bf3dee21eef0d6148290327ad0. M4-B is split into the persisted model, shared rendering, and runtime reference/rename integration. The typed additive chart options use finite numeric admission, stable monotonic reference-line IDs, and styles keyed to typed series identities. Default options are absent from serialized legacy charts and retain the previous configuration. Rendering and runtime option behavior are the next two parts.

cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E with test(chart_options_roundtrip_with_legacy_defaults_and_reject_invalid_wires) | test(chart_reference_versions_preserve_physical_rows_and_validate_named_wire) | test(chart_bar_label_admission_matches_vector_capacity) | test(chart_definition_roundtrip_and_rename_history_keeps_independent_selection) | test(named_chart_references_follow_rename_history_and_independent_edits) | test(legacy_recovery_defaults_options_and_future_record_is_preserved) passed 6 of 6. Independent review found no substantive model or compatibility issue. Staged diff whitespace verification passed.


## M4-B2A axes in the shared scene

M4-B1 was pushed at a69e133993004461e0d358ba1920a2bc301b27d6. One implementer could not start the original combined slice. A fresh implementer supplied and verified the axis transform foundation, then the orchestrator integrated it in the existing definition-aware scene. Fixed/log axes, bounded ticks and gridlines use the same scene for paint and raster. The legacy default path produces identical raster output. The exact fallback clips rare extreme crossings before conversion to display coordinates, with a 256-crossing cap per scene. Tick labels preserve adjacent values, omit colliding labels, and identify underflowed log positions as outside the representable range. Review noted custom Y-label fit as a presentation follow-up for B2B.

Initial integrated frozen-source run passed 9 of 9 in 0.348s after a 3m29s build.
cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(axis_transform_) | test(chart_options_route_log_axes_and_reject_nonpositive_data) | test(chart_options_clip_extreme_crossings_without_dropping_raw_points) | test(chart_options_bound_ticks_and_preserve_legacy_rendering) | test(vector_validation_and_extreme_transforms_stay_bounded) | test(chart_bar_ticks_include_the_group_inset) | test(chart_adapter_reports_length_errors_and_uses_only_current_snapshots) | test(chart_copy_uses_scene_and_surfaces_clipboard_failure)'

Final focused rerun passed 5 of 5 in 0.223s after a 1m53s build.
cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(chart_options_route_log_axes_and_reject_nonpositive_data) | test(chart_options_clip_extreme_crossings_without_dropping_raw_points) | test(chart_options_bound_ticks_and_preserve_legacy_rendering) | test(chart_options_ticks_distinguish_close_bounds_and_hide_overlapping_labels) | test(chart_options_zoom_does_not_label_underflowed_log_tick_zero)'

Independent review confirmed the tick corrections. Existing test-build warnings remain in vendored and unrelated source. The shared appearance and reference-line work remains M4-B2B; runtime resolution, identities and freshness remain M4-B3.


## M4-B2B shared appearance and reference geometry

M4-B2A was pushed at b15ac8822ffe25551c0e72d7643aa722305b1559. Chart title, legend placement, per-series color/style/marker/thickness and literal reference lines are part of the shared scene. Reference geometry accepts named values through a typed ID-to-value seam for B3. Dash phase carries through dense connected segments, and axis label formatting respects display bounds and log scale.

The initial focused fast-dev library Nextest passed 13 of 13. After independent review corrections the final focused run passed 14 of 14, covering dense dashes, scale-aware log labels, new shared raster/reference primitives, and legacy parity. Review rechecked both findings, and diff whitespace verification passed.


## M4-B3A current named reference values

M4-B2B was pushed at 721334455e155cb5b633cca37eb3b4e3aa006029. Its initial and final commands were cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::chart::tests::chart_)'.

The Charts companion now resolves labeled named lines from current committed evaluation bindings, passes resolved IDs into the shared scene, and includes the matching evaluation basis when a sensitivity chart uses named values. Cached scene and Copy actions are fenced against stale data, definitions and ownership. The default and literal-only routes retain their existing behavior. Rename and style lifetimes remain M4-B3B.

cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(gui::calculation_chart_dialog::tests::chart_)' passed 9 of 9. Independent review found no substantive freshness issue. Diff whitespace verification passed.


## M4-B3B chart identity and rename

M4-B3A was pushed at de7fe59f9a3e7491b481df8ec028567bdce93788. Document rename and replay rekey styles by exact chart output identity and bind named reference edits to admitted reference-line IDs. Independently edited styles and labels retain their values. Collision-hidden styles follow the document/output identity across history entries. Source-kind replacement and chart deletion retire hidden identities. This closes the M4-B option model, render and runtime work. Controls and distribution display remain M4-C.

Both focused Nextest runs used cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::session::tests::chart_) | test(calculation_pad::session::tests::named_chart_references_) | test(gui::calculation_chart_dialog::tests::chart_owned_popup_menus_)'. Initial 6 of 6 and final 7 of 7 passed. The final run covered the two resolved review findings for chained rename and source replacement/deletion. Diff whitespace verification passed.


## M4-C1A distribution definition and ownership

M4-B was pushed at 21d1ab7d893dbfc36e5ee1bdff7c9fc4da3bea55. The additive distribution definition owns one typed vector reference and retains ordinary chart selections. Its rename/undo scope works even while dormant. ECDF data is independent of whether a histogram width would be feasible. Under this checkpoint, the new distribution kinds have no controls and their renderer is pending; they are not yet delivered features.

cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::chart::distribution::tests::) | test(calculation_pad::persistence::tests::distribution_definition_) | test(calculation_pad::persistence::tests::chart_options_roundtrip_) | test(calculation_pad::session::tests::chart_distribution_source_)' passed 8 of 8. Independent read-only review found no substantive compatibility or ownership defect. Diff whitespace verification passed.


## M4-C1B cumulative distribution scene

M4-C1A was pushed at 5b8ef82e544697c35495c4ab588cddd80b6dd425. The cumulative distribution scene uses the real current vector, groups duplicate thresholds, renders the zero and one plateaus and exact empirical jumps, and shares raster output and evaluation freshness with Charts. Plot hits expose group and cumulative counts. Histogram rendering and controls remain subsequent slices. Style admission for a distribution source separate from the ordinary source still needs integration in controls.

cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::chart::tests::cumulative_scene_) | test(gui::calculation_chart_dialog::tests::cumulative_adapter_) | test(calculation_pad::chart::tests::chart_options_) | test(gui::calculation_chart_dialog::tests::chart_named_)' passed 9 of 9. Independent review found no substantive cumulative scene, evaluated data or copy issue. Diff whitespace verification passed.


## M4-C1C exact histogram scene

M4-C1B was pushed at 2f2e08bbc1122c4f0fc1b0ae304b15986ea1ba3e. Exact interval endpoints drive the shared scene, including the nominal final edge above the finite scalar range. The current-vector adapter selects Count or Percentage. Fixed display bounds clip geometry while retaining complete counts and interval membership. The reviewed fixes extend hover across the clipped bar, show exact representable-value membership, render valid log ticks over the finite portion of its exact domain, and exclude offscreen constant anchors. An interior shared-pixel-edge right-bin tie-break remains a small C2 follow-up.

The initial frozen run exposed an assertion that treated zero-count-bin hover as unexpected. It was corrected to check the two nonempty bins and bounded zero-bin behavior. The final frozen-source command cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::chart::tests::histogram_scene_) | test(gui::calculation_chart_dialog::tests::histogram_adapter_) | test(calculation_pad::chart::tests::cumulative_scene_) | test(calculation_pad::chart::tests::chart_options_) | test(calculation_pad::chart::distribution::tests::)' passed 14 of 14. Diff whitespace verification passed. Companion controls remain pending.


## M4-C2A1 chart keyboard ownership

M4-C1C was pushed at 413fc31a25dc670decc28d17d73aca4b1ea80f92. Its initial histogram run passed 11, failed one incorrect assertion about zero-bin hits and canceled two. The same filter passed 14 of 14 after correction.

The existing Jump Mode coordinator now recognizes docked and native Charts and registers the visible controls. Series targets follow the source identity across reorder. Selecting a hint focuses without editing the chart. Histogram hover favors the right bin at a shared interior pixel edge.

The new retained app fixture initially overflowed the Windows test stack, so it runs on a 16 MiB test thread and disables both Paste Sequence shortcuts before application construction. One follow-up fixture clicked the chart collapse control. Registration diagnostics showed the layer was correctly owned while the collapsing content clipped its controls. The corrected title interaction passes and temporary instrumentation was removed.

Final frozen-source cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(gui::render::tests::chart_jump_targets_follow_docked_and_native_owner) | test(gui::jump_mode::ui::tests::expanded_dialog_layer_inventory_matches_actual_window_ids) | test(calculation_pad::chart::tests::histogram_shared_edge_hover_prefers_right_bin) | test(gui::calculation_chart_dialog::tests::chart_owned_popup_menus_) | test(gui::calculation_chart_dialog::tests::chart_series_jump_identity_)' passed 5 of 5. Diff whitespace verification passed.


## M4-C2A2 distribution controls and summaries

M4-C2A1 was pushed at 5bce6c718b18e7d029c457ecfcedba2f16adb977. Histogram and Cumulative now have real controls within Charts. The selected numeric vector and validated bin/scale settings are persisted while ordinary selections remain available. Invalid current widths cannot replace committed bins. Summary rows are virtualized, owner-gated, snapshot-bound and keyboard-selectable. Drafts follow the owning Pad document. Two related follow-ups remain in C2B: retiring queued summary focus on collapse/owner loss and wrapped selected-row details at narrow widths.

Initial Nextest passed 5 of 5. Final focused cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(gui::calculation_chart_dialog::tests::chart_distribution_) | test(gui::calculation_chart_dialog::tests::histogram_width_change_) | test(gui::calculation_chart_dialog::tests::chart_owned_popup_menus_) | test(gui::calculation_chart_dialog::tests::cumulative_adapter_) | test(gui::calculation_chart_dialog::tests::histogram_adapter_) | test(calculation_pad::chart::tests::cumulative_scene_)' passed 7 of 7 after review corrections. Diff whitespace verification passed.


## M4-C2B1 staged chart axes and layout controls

M4-C2A2 was pushed at a39bd1561ed7e3b4d5755425f4a9e6ede31b8497. Chart Options exposes separately staged axes, log scales, bounds, ticks, grid, title and legend. Reset Axes leaves the transient view intact. Unrelated chart changes preserve the staged draft. Definition-only restrictions remain enforceable with a stale evaluation. Summary collapse or owner loss retires queued focus before the animation, and the selected row has a wrapping detail.

The first build exposed a test-only Context borrow error. The next run passed seven cases, then its animated panel click failed and two cases were canceled. Stabilizing the fixture preserved its behavioral assertions. Final cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(gui::calculation_chart_dialog::tests::chart_options_are_staged_) | test(gui::calculation_chart_dialog::tests::chart_distribution_) | test(gui::calculation_chart_dialog::tests::histogram_width_change_) | test(gui::calculation_chart_dialog::tests::chart_owned_popup_menus_) | test(calculation_pad::chart::tests::chart_options_)' passed 10 of 10. Independent recheck confirmed all three fixes. Diff whitespace verification passed.


## M4-C2B2 chart references and series appearance

M4-C2B1 was pushed at e29ddef9628e956d48c694bce12f228738310797. Chart Options now stages labeled literal and current named reference lines, and per-series color, line/marker style and applicable thickness. Stable reference IDs and keyboard-accessible scalar search are owner-bound. New named lines require an actual current scalar and positive log coordinates; old unavailable bindings remain editable. Distribution styles use an independent typed identity, including across rename/undo collisions. Dormant styles survive compatible mode switches. This closes M4-C implementation; interactive Windows evidence is still pending.

The focused run passed 12 of 12 after substantive review. The final narrow correction rerun passed 2 of 2 using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(gui::calculation_chart_dialog::tests::chart_options_reference_and_style_widgets_apply_and_keep_ids) | test(gui::calculation_chart_dialog::tests::chart_options_reject_new_nonpositive_named_log_reference_with_unresolved_retained_reference)'. Diff whitespace verification passed.

## M1–M4 combined Windows smoke

The 2026-11-17 Eastern run built pushed SHA bc356a0ebc0cd512e5b18b3968f6e476b6d2ef11 with cargo build -p multi_launcher --bin multi_launcher --profile fast-dev --locked. The build exited 0. Binary SHA-256 was F5FABAC797683E5A52C835DA54700A1A27BAE752DDFDCEC752D99D5A00AE5439. The app ran with the isolated target/productivity-toolkit-expansion-evidence/m1a settings and both Paste Sequences shortcuts disabled.

- ZIP: the real launcher command opened ZIP Extractor. Keyboard focus reached Choose ZIP, Enter opened the Windows Open picker, and Escape returned to Choose ZIP. The disposable folder had no extraction destination after cancellation. The automation could see the filename field in the parent accessibility tree but could not address it there, and the native modal was unavailable as a separate target. Selecting a ZIP, reviewing its proposed path and confirming extraction are covered by the targeted Windows tests, with interactive evidence incomplete.
- Pad: launched calc pad and typed the synthetic source from calculation-fixture.txt. The real editor reported ordered [30, 20, 15, 10], doubled [20, 40, 30, 60], trailing [15, 17.5, 22.5], percentile 13.75 and regression slope 2.
- Charts: opened from the actual Pad Configuration → Tools menu. Keyboard navigation exposed staged Gridlines, Legend, Reference lines, Series appearance, Apply, Cancel and Reset Axes controls. Histogram showed two bins of two observations each, and Cumulative showed four endpoints with 25%, 50%, 75% and 100%. Pop Out created the actual Charts — 1 window. Its captured accessibility tree exposed the window frame only, and window image capture timed out on both attempts. The targeted tests provide the evidence for viewport state, plot primitives, log/reference behavior and retained chart interaction. Popup content and copied image appearance were not inspected interactively.

The M4-C2B2 12-case Nextest expression was test(gui::calculation_chart_dialog::tests::chart_options_) | test(calculation_pad::session::tests::chart_distribution_style_) | test(calculation_pad::session::tests::chart_styles_and_reference_ids_) | test(calculation_pad::session::tests::chart_hidden_style_) | test(calculation_pad::persistence::tests::chart_options_roundtrip_) | test(calculation_pad::persistence::tests::distribution_style_roundtrip_) | test(calculation_pad::chart::tests::chart_appearance_) | test(calculation_pad::chart::tests::histogram_distribution_style_), using the same productivity settings as the earlier run.


## M5-A1 executable vector reference foundation

M4-C2B2 was pushed at bc356a0ebc0cd512e5b18b3968f6e476b6d2ef11. The typed catalog draws callable spellings and signatures from the shared evaluator inventory. It covers all 27 vector transformation, callback and product calls, with argument ranges, expectations, stable related IDs and name/intent ranking. It reuses the established executable-example assertion. The original implementer handed back the checkpoint without changes, and the root finished it as sole writer. Independent review found no actionable issues.

Windows Nextest passed 4 of 4 (including the grouped legacy examples) using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::reference::tests::reference_) | test(shared_examples_evaluate_to_their_documented_results)'. The Rust compile took 11m 21s. Process counters confirmed resumed output during build diagnosis, so compilation completed without cancellation. Individual tests took 0.288s in total. Diff whitespace verification passed. The rest of the language and the retained help UI are separate pending work.


## M5-A complete executable API reference content

M5-A1 was pushed at 934997f887ecb2b5cc404cad63eaa6c41db69f89. The complete offline content has all 119 evaluator function names, with executable typed expectations and a check for the actual arities of fixed overloads. It covers operator spellings, statements, value forms, chart and Sensitivity usage, limits, and all 17 authoritative unit categories and their aliases. Existing grouped examples and their evaluator checks remain in place. Search ranks exact name and alias ahead of incidental matches. Every related ID resolves.

Initial compile exposed a unit macro fragment restriction and was corrected. Both complete targeted runs passed 6 of 6 with cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(calculation_pad::reference::tests::reference_) | test(shared_examples_evaluate_to_their_documented_results)'. The final compile took 2m 03s and its six tests took 0.588s. Independent recheck closed the material findings. Two final description-only details were aligned with existing Cumulative behavior (empty vector and linear Y); the same catalog tests will be included in the M5-B run. Diff whitespace verification passed. M5-B owns user-visible navigation and insertion.


## M5-B1 safe prepared insertion

M5-A was pushed at 1dbe374c17aab7f1d19b4447c7fbc1460ddcee82. Prepared examples admit UTF-8-safe top-level positions and complete selections without replacing a whole nonempty draft. The existing Pad source and rename analysis supplies contextual declarations, calls and physical row evidence. Preparation preserves surrounding bytes, flags names or physical references that would change, and keeps examples with their own physical references at the start of the document. This checkpoint does not mutate a draft. The next checkpoint wires preparation into retained help actions and normal Pad Undo.

The initial compile exposed access to a private parser diagnostic. The corrected catalog-inclusive run passed 9 of 9 with the selectors below plus test(calculation_pad::reference::tests::). The final run after independent review passed 4 of 4 with cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(insertion_geometry_and_collisions) | test(insertion_detects_physical_rows_and_local_formula_calls) | test(row_named_formula_is_distinct_from_physical_row_reference) | test(document_rename_preserves_units_keywords_comments_and_physical_rows)'. Review recheck found all three behavior corrections in place. Diff whitespace verification passed.


## M5-B searchable Reference and safe insertion

M5-B1 was pushed at c25dfcc0db7d511578c2c835283019d62620db5c. The existing Examples & Reference companion now has an API Reference tab beside Grouped Examples. It retains current search, filters, selected detail and scroll across its hosted presentations. Reference browsing has no evaluation, source or clipboard side effects. Copy uses the existing retained copy route. Explicit Insert validates the current Pad owner, caret, source and fold state, reviews collisions and physical rows, and writes through the normal undo history. Exact line-splice identity keeps an existing row stable when the inserted sample is identical. Manual mode does not start a calculation. Moving to another entry invalidates an outstanding review.

The first compile corrected the test profile's data-root type. Independent review led to exact sample confirmation, real foreground ownership, one-shot keyboard focus reveal, and line identity preservation. Initial ownership testing exposed a retained-layer registration issue. Press/hover evidence isolated stale Pad focus disabling the visible Reference header. The corrected foreground-layer gate passed all 18 cases, including the existing preservation, native-host, grouped example and narrow-window fixtures, in 3.910s after compilation. Final command: cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(gui::calculation_pad_dialog::tests::reference_) | test(gui::calculation_pad_dialog::reference_help::tests::reference_api_) | test(calculation_pad::reference::) | test(shared_examples_evaluate_to_their_documented_results)'. Diff whitespace verification passed.

Native Windows smoke on 2026-11-17 Eastern built with cargo build -p multi_launcher --bin multi_launcher --profile fast-dev --locked (exit 0, 4m 53s). Binary SHA-256 280DC838D0FC3D21E1FFBAC6F1AEDCDD729388C9A2439DF4BCFDFFD6D9EB4B7C. Both disposable profiles explicitly disabled Paste Sequences Next and Cancel before launch. The real calc pad command opened the draft, and Config Menu → Tools → Examples & reference opened the docked companion while retaining the synthetic source. A separate profile with remembered Pop Out opened an actual Calculation Pad Examples & Reference window and retained the draft. The docked accessibility tree omitted the companion's children at its child limit, window capture timed out, and the popped-out snapshot exposed the window frame only. Native insertion/Undo and direct query refocus were not observed. The retained UI tests verify actual widget search and navigation, owner fencing and the insertion/Undo path. Carry native keyboard/focus touchpoints into M12-B. Both disposable launcher processes were stopped.


## M6-A source-preserving Markdown section moves

M5-B was pushed at 9e2148ba8e01d7b31767931b7989bc232f7311c7. The shared parser now recognizes top-level ATX and Setext headings with exact physical ranges. Section hierarchy is built in one pass. The prepared move rotates one bounded source span, consumes the selected subtree and admits only same-parent placements whose reparsed structure preserves parent identities. Unnamed structural sections keep their own boundaries without becoming public outline rows. Preamble, paired metadata, heading indentation, CRLF/LF, and final-newline presence survive moves. A review warning includes both renamed duplicate anchors and changes to the first resolved target of a colliding anchor.

The initial targeted run exposed the empty-ATX regression. Later corrections handled real base-text seams, sparse hierarchy, unnamed boundaries, Unicode Setext and thematic rules. The final focused Windows Nextest run passed 26 of 26 using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(notes_markdown::section_move::tests::) | test(notes_markdown::parse::tests::) | test(notes_markdown::headings::tests::)'. Independent review verified structural corrections and supplied the final Unicode classifier case, now covered. Diff whitespace verification passed. M6-B owns retained Notes and Studio controls.


## M6-B Notes and Studio outline interactions

M6-A was pushed at 44b8d27166346ddaec403da5333dafde4037cc4d. Both editors now render a drag insertion line only for an admitted same-parent slot and offer keyboard-accessible Move up and Move down. Invalid owned drops show the refusal reason. Moves use the complete source hierarchy, remain bound to the current document, source revision, host and viewport, and show an exact confirmation for changed heading links. The retained edit updates selection, reveals the moved row without taking editor focus, and creates one normal Undo boundary. Notes retains ordinary dirty and save behavior. Studio rejects a move if source editing later in the same retained frame changes the draft. Both editors preserve focus on keyboard outline selection.

The initial targeted run passed 14 cases and exposed an idle Studio test fixture that supplied no pending action. The corrected retained fixture and reviewed adapters passed 16 of 16. The final prompt-ownership and isolated test-profile correction passed 16 of 16 in 0.948s using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(outline_move_) | test(notes_markdown::section_move::tests::) | test(outline_rows_respect_max_depth_filter) | test(outline_state_is_hidden_and_pending_navigation_ignored_when_disabled)'. Actual egui mouse press/drag/release exercises a valid slot, an invalid slot's feedback and rejection after revision change. Notes and Studio fixtures exercise normal Undo. A first-run generated default config was moved to the ignored evidence directory and the final test fixture generated no root file. Diff whitespace verification passed.

Standalone Windows move and Undo in each editor, including a pop-out, remain pending. Combine that interaction with the M7-B Notes/Studio smoke to avoid a redundant standalone build and preserve explicit native evidence.


## M7-A1A page geometry and embedded font shaping

M6-B was pushed at 5c973f5e7ecd66669735e8ef1b0ba8fadcb83d85. The shared print foundation validates point-based paper, orientation, margins, text sizes and single-line header and footer templates. The author supplies exact date and page counts. Shaping reuses the launcher's embedded Ubuntu Light, Hack and Noto Emoji via epaint_default_fonts and Rustybuzz, with explicit bidi and script handling. Font IDs, actual glyph positions, ink bounds and original UTF-8 clusters are available to the shared paginator and output adapters. Missing graphemes produce visible replacement and an exact range warning. The calling layout expands tabs before shaping.

The initial focused offline run passed 2 of 2 and confirmed all dependency versions were already in Cargo.lock. After independent review, canonical coverage, nonprinting bidi controls, original cluster ranges, ink overhang and primary line-height checks were verified again by independent inspection. The final focused run passed 2 of 2 after a 1m 57s compile. Command: cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(notes_markdown::print_layout::tests::)'. Diff whitespace verification passed. M7-A1B adds the shared Markdown page scene and actual pagination.


## M7-A1B1 bounded print Markdown parser

M7-A1A was pushed at 61fdcaef2bf17db0aca3df25ff53f51a2b63ffd3. The print parser uses a flat bounded arena from the existing offset-aware Markdown event stream. It retains rich block and inline structure, source ranges and existing anchor identities. It applies the shared Notes callout normalization and returns the exact consumed header source range. Print page breaks require a canonical physical source line authenticated by a complete top-level HTML block. Existing paired metadata and nested, inline, escaped, fenced and larger HTML/comment constructs cannot introduce breaks. Renderable unsupported HTML remains readable warned source. Source bytes, events, nesting, owned text and cancellation are checked.

Independent review identified an overlapping-delimiter panic, empty named anchors and divergent callout handling. The corrected parser passed independent recheck. Focused run passed 3 of 3 after a 3m 52s compile in 0.164s using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(notes_markdown::print_layout::markdown::tests::print_markdown_)'. Diff whitespace verification passed. M7-A1B2 positions text and tables into pages and M7-A1B3 resolves bounded local raster images and Mermaid.


## M7-A1B2 text and table pagination

M7-A1B1 was pushed at e4d53751b85f96688c9f3ad4b438b4a088b6576b. The shared point-based page scene now places selectable shaped text, links, rules, title, headers and footers with actual page counters. It wraps code with its own configured font size, uses translated glyph ink and common line baselines, and records heading destinations after actual placement. The layout preserves ordered list children and constrained nesting, formats callouts from the existing header evidence, repeats continuation table headers and splits long cell rows. Tables that cannot fit use a readable labeled fallback, including header-only tables. Work, retained shape payloads and link annotations are bounded, share immutable glyph/data payloads, and check cancellation. Unsupported font, link, asset and mixed or wrapped directional cases disclose their limitations.

Two read-only reviews found and rechecked memory, exact-fit rounding, ordering, ink, decoration, and link-placement defects. The final correction passed 7 of 7 after a 4m 12s compile with 1.353s of tests using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 -E 'test(notes_markdown::print_layout::document::tests::paginate_)'. Diff whitespace verification passed. Real local images, Mermaid and completing keep-with-next for compatible following tables and raster blocks remain in M7-A1B3.


## M7-A1B3 bounded local images and Mermaid in the page scene

M7-A1B2 was pushed at 80237609eef648f17908075760294baf19acc86f. The same page model holds immutable straight-alpha raster data and exact figure/link rectangles. It admits only existing local-relative raster destinations through the supplied editor resolver. It checks occurrences and resource budgets before expensive operations, charges failed encoded and decoded attempts, caches outcomes for borrowed aliases and SHA-256 plus exact-byte matches, and caches admitted width per destination. PNG decode starts with explicit limits. A bounded chunk walk checks the existing CRC and omits unused ICC metadata only from decoder input to avoid inflation before allocation accounting. Original asset bytes remain untouched. Mermaid uses the existing resource-free bounded renderer. Unsupported images, table images, and Mermaid errors have readable fallback and warnings. Inline figures stay ordered in paragraphs, headings, and tight and loose lists. Heading fit uses the actual following table or raster size and keeps image-first headings with their following text where space permits. Labeled table fallback streams rows and measures only its first line for keep behavior.

The initial focused run passed 5 and failed one fixture whose filler did not force a page migration. The corrected layout and asset suite passed 11 of 11 after a 2m 02s compile in 4.627s with cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(notes_markdown::print_layout::document::tests::paginate_) | test(notes_markdown::print_layout::assets::tests::)'. Independent reviews verified and drove additional encoded/decoded-work, table expansion, PNG metadata, and image-first heading corrections. The final two changed-case tests passed after a 2m 01s compile in 0.810s using the same flags and -E 'test(png_icc_chunk_is_checked_and_removed_before_decoding) | test(paginate_local_images_deduplicate_preserve_width_and_inline_order)'. Diff whitespace verification passed. Actual PDF encoding, publication and native preview/print follow.


## M7-A2A selectable PDF encoding

M7-A1B3 was pushed at 7ed7363d20115ba38823e6c3a7b59de9935538fb. The encoder consumes the same immutable point scene, writes selectable text with licensed subset embedded fonts, and places the existing vector, styled glyph, and raster content. It writes safe URI and exact internal page annotations. Raster identities include their dimensions and preserve straight alpha. Italic transforms stay balanced on cancel. Shared styled layout measurements include the same round bold stroke and italic shear that the PDF uses. The encoder meters expanded glyph, cluster, URI and asset work before materializing it, and enforces the final output limit. The print scene rejects overlong normalized links with a warning.

Cargo metadata with --format-version 1 --filter-platform x86_64-pc-windows-msvc resolved pinned Krilla 0.8.2 with raster-images only and 10 new transitive packages. No existing locked package version was upgraded. Independent reviews found and corrected transform-cancellation, shared style geometry, raster cache identity and expansion budget issues. The initial focused run passed 12 of 12, and the final run passed 13 of 13 after a 2m 02s compile in 2.876s using cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(notes_markdown::print_layout::pdf::tests::) | test(notes_markdown::print_layout::document::tests::paginate_)'. The exact MULTI_LAUNCHER_TEST_PDF_PATH final override was target/productivity-toolkit-expansion-evidence/m7-a2a/scene-final.pdf. Diff whitespace verification passed.

Independent bundled pypdf strict reading and pdfinfo confirmed three Letter pages at 612 by 792 points, 24,963 bytes, all 100 selectable table rows in order with repeated header and actual page counters, selectable accents, emoji and monospace, embedded Hack Regular, Noto Emoji and Ubuntu Light with ToUnicode, and the exact external URI and first-page internal XYZ destinations. Two different-sized image objects, 2 by 8 and 4 by 4, have their exact 96/255 alpha samples. Poppler pdftoppm at 120 DPI rendered pages 1 and 2, which were visually inspected for legibility, placement and table continuation. Final synthetic PDF SHA-256 afbcb53b04c7c8586ef94b5c9cc75c42be1f7f2dce8f99c307a72cd876ceb298. Actual editor Save As and native print proof belong to M7-A2B and M7-B.


## M7-A2B guarded PDF destination publication

M7-A2A was pushed at c59241d1241adf7111d19c61de08ebb3ad96fecc. The publication backend reviews one selected literal local PDF leaf and binds approval to its held-parent identity. Existing files additionally require exact native identity, supported metadata and a bounded content fingerprint. Source checks protect the active path including hardlinks, relative Notes origins and a missing leaf under an existing parent. On Windows the worker stages an exact write under no sharing, copies matching owner, group and DACL before any replacement bytes, and refuses alternate streams or unsupported protections and attributes. Existing archive callers keep their original access choices. New output uses relative no-replace publication. Approved replacement moves the exact original to a sibling backup, publishes the exact stage without replacement, and restores the original by its held handle if publication or late admission is revoked. A third-party occupant is preserved and recovery reports the retained backup or stage location and identity when known.

Independent reviews found and closed source sharing, ACL-before-bytes and pre-open races, exact reselection, alternate path spelling and confirmed fingerprint issues. Windows focused publication run passed 9 of 9 after a 4m 12s compile in 0.292s. After narrowing the archive facade, the final union passed 11 of 11 with cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(pdf_publication_) | test(c06_root_relative_publication_commits_exact_bytes_without_replacement) | test(c06_stage_identity_failure_reports_cleanup_and_preserves_foreign_files)'. Covers actual Windows temp files, active hardlink, late foreign occupant, changed parent and destination revision, unsupported properties, early stage reads and cancellation/rollback. The alternate-stream case is conditional on the filesystem admitting one. Diff whitespace verification passed. M7-B connects both editor commands, preview and the native printer workflow.


## M7-B1A shared page raster renderer

M7-A2B was pushed at e5d6cd47ed2a37d0e73be8c4bb5c6e9e9149e705. The shared renderer consumes the exact point scene used by PDF and returns one bounded opaque top-down RGBA page with its actual scale. It creates paths from the same embedded face and glyph IDs, applies prepared glyph positions and the same round bold stroke and italic shear, and respects contour winding. It composites immutable raster assets in scene order and draws rules and strike geometry once. The renderer checks dimensions and cumulative pixel/work limits, meters both bold paint passes, and checks cancellation through text runs and pixel preparation.

Independent review confirmed PDF alignment, outline support for all three fonts, compositing and transform conventions. The corrected focused run passed 3 of 3 after a 3m 01s compile in 0.220s using cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(notes_markdown::print_layout::raster::tests::)'. Tests exercise glyph counters, styled margin bounds, top-down asymmetric alpha, capped scale, invalid inputs and cancellation during replay. Diff whitespace verification passed. M7-B1B connects retained editor preview and Save PDF. M7-B2 submits the same pages to a real Windows device.


## M7-B1B1A retained preview worker and asset loading

M7-B1A was pushed at 126a18999faa19e78b5de0cf651bb94651e2ada0. The shared worker keeps one serial-linearized running request, one latest pending slot and one result. Current scene receipts hold the typed editor stamp, exact draft, allowed asset base, settings and date. Page or zoom rendering reuses the accepted scene. New requests and retirement cancel obsolete work, and completion requests repaint of the owner and root. The worker validates source size and geometry before filesystem work. Windows print assets walk retained local directories relative to the opened root, then read the same validated ordinary file through the held chain with byte limits and cancellation. The Studio legacy loader retains accepted-format byte accounting. Its bounded read closes file growth races.

Independent review addressed lost latest requests, read and metadata races, cancellation during chunked reads and legacy HTML compatibility. The first compile found PrintError needed Display; the domain now owns its readable form. The corrected focused run passed 10 of 10 after a 4m compile in 0.461s using cargo nextest run --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(markdown_print_worker_) | test(note_html_export_reads_bounded_assets_and_preserves_width_and_warnings) | test(note_html_export_budgets_bytes_read_even_for_rejected_assets) | test(markdown_studio_preview_uses_bounded_document_relative_assets) | test(markdown_studio_print_assets_charge_attempts_without_changing_legacy_accounting)'. Diff whitespace verification passed. The forthcoming retained host will create the worker lazily, keep its slot across reopen, and accept exact current receipts.


## M7-B1B1B retained Notes and Studio print preview

M7-B1B1A was pushed at 0b7028dff2de5a4dd46ea08b34a2a41a901edd8d. Both editor menus now open the hosted Export / Print preview from an accepted current draft. Studio applies the same-frame editor change before handing off the open. Notes refreshes its same-panel draft, including a first saved path. The preview registers the actual native utility key, participates in visible and hidden-root completion, tracks the current parent document and clears stale scenes on edits, layout changes or retirement. It provides actual page settings, title and counter tokens, page navigation, zoom, Fit Page and warnings. Page and zoom reuse the accepted scene. The preview worker starts lazily and prepares the upload pixels off the UI thread. The retained viewport supplies numeric DPI and texture limits. Root navigation follows keyboard ownership, and initial fallback focus respects a control already focused by the user. The final editor Save PDF and native print actions follow in B1B2 and B2.

The assigned implementer returned without source changes on this integration, so the orchestrator completed it sequentially. Independent reviews identified native geometry registration, hidden-window lifetime, Fit stability, viewport DPI, texture handling and keyboard ownership issues. The initial targeted run passed 11 of 11. After the final keyboard check and rendered Fit fixture, the final targeted run passed 12 of 12 in 1.106s after a 2m 07s compile. Exact command: cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(markdown_print_) | test(utility_dialog_sizes_keep_docked_and_native_geometry_independent)'. The headless app test used a disposable profile and disabled both Paste Sequences shortcuts before construction. Diff whitespace verification passed. Standalone native editor/Save/print verification remains assigned to B2.


## M7-B1B2 guarded Save PDF from the accepted preview

M7-B1B1B was pushed at 0228db6c03ebc4927277fe075ce9e12350216f24. The Save PDF action completes only from the retained preview host and compares the exact accepted scene with the live editor draft before and after its owned native Save As chooser. The output worker reviews the chosen path, exposes a separate exact replacement confirmation when required, and encodes the accepted scene without saving the source. Confirmation starts with focus on Cancel, and Escape follows the shared viewport's keyboard ownership. The worker couples revocation with one final atomic admission after native stage validation, handles original-file restoration on refusal, and reports the actual outcome when a save crossed admission before the window closed. Pending work remains visible to hidden-root completion.

The assigned implementer declined this integration without source edits, so the orchestrator completed it sequentially. Independent review prompted the admission, late completion, root lifetime and keyboard changes. Windows tests exercise actual new/replacement PDF bytes, replacement cancellation, refusal at final admission with clean restoration, active-source and native no-replace guards, and exact retained app action acceptance. The app fixture used a disposable profile and disabled both Paste Sequences shortcuts. Separate launch and editor Save As proof is scheduled with B2B.

The first compile exposed an import through the private publication module; corrected through its public facade. The final focused Nextest run passed 26 of 26 in 2.047s after a 3m 53s compile. Exact command: cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(markdown_print_) | test(pdf_publication_) | test(c06_root_relative_publication_commits_exact_bytes_without_replacement)'. Diff whitespace verification passed.


## M7-B2A native GDI printer adapter

M7-B1B2 was pushed at 91250e64c7d566893624542aa0d934e24321a999. The native adapter owns a unique printer DC and the exact immutable prepared scene. It validates device capabilities, page selection and dimensions. Actual Size uses the device hard-margin offsets. Fit centers a uniform physical scale in the printable rectangle and calculates potential clipping from the rounded destination GDI receives. It renders one bounded top-down opaque BGRA page at a time from the same scene as preview and PDF, and admits the job immediately before StartDoc. It reports Windows submission after successful EndDoc. Cancellation checks span raster preparation and native page boundaries, and failure after StartDoc attempts AbortDoc once. The scoped callback reports spool exhaustion and user/application cancellation accurately.

The independent review identified drawable clipping, device capability, late source revocation and GDI callback issues. Initial focused verification passed 3 of 3 with real Microsoft Print to PDF job 2. The corrected run passed 4 of 4 after 1m 59s including a real worker-thread DC handoff and job 3. Exact focused command: cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast --run-ignored all --success-output immediate -E 'test(notes_markdown::print_layout::printer::tests::)' with MULTI_LAUNCHER_TEST_PRINTER_PDF set to a fresh absolute synthetic workspace path. The Windows driver produced target/productivity-toolkit-expansion-evidence/m7-b2a/virtual-1dabe0c47728414c8cc01ca86e72fb58.pdf, 303382 bytes, SHA256 0C35E709364052E78F2D91B4789ABD035A7A5C2FDB2767119C1AEDFCAD172156. Poppler and the strict PDF parser found two Letter pages. Both pages rendered with the expected distinct synthetic text and were visually inspected. This proves direct spooling at the adapter boundary. B2B adds accepted chooser ownership, app confirmation and native editor workflows. Diff whitespace verification passed.


## M7-B2B integrated Windows print and editor smoke

M7-B2A was pushed at 480bf760f68d3112ba40a4ff038966e819ca85e6. The retained preview owns Windows printer selection and validates the HWND and exact scene around native modal completion. Options preserve the selected device, page ranges, driver copies and collation. The same worker holds the configured DC through an explicit review with actual paper dimensions, hard margins, physical scaling and clipping warnings. Print and Save PDF share one output slot. Cancel and source or host retirement revoke unapproved work. The worker reports a submitted Windows job ID and any relevant late failure. The independent review led to revalidation of the native HWND after the modal and exclusion of duplicate Copies and Collate controls in driver property pages.

The first compile exposed one stale test-helper type name. The corrected focused run passed 20 of 20 in 1.968s after a 4m 51s compile using cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(markdown_print_) | test(printer_chooser_) | test(printer_placement_) | test(printer_callback_)'. The ordinary desktop build succeeded in 3m 50s with cargo build -p multi_launcher --bin multi_launcher --profile fast-dev --locked. Binary SHA256 F124D734A76307EC10D866A373A0C07AA76EFA8C08E9864774C08F9733A5413C. Diff whitespace verification passed.

Actual Windows Notes and Studio smoke used only the synthetic profile with Paste Sequences Next and Cancel disabled. In Notes, the real launcher command opened Print Smoke Note. An extra unsaved sentence remained present through keyboard Move down of Brief past Appendix and Ctrl+Z returned the original section order. In Studio, the real launcher command opened an unsaved two-page draft. Its outline moved First Section after Second Section and Ctrl+Z restored the original sequence and earlier text. Both moves and undo were performed in docked editors. Studio also opened an actual native Markdown Studio window. The accessible native window exposed only its frame and the launcher inventory did not expose its controls. Moving a section in a popped-out editor remains unverified.

Both dirty editors opened their live two-page Export / Print previews and a Windows Save As dialog with the PDF filter. Escape canceled both. Notes also opened the live Windows printer chooser with the installed Microsoft Print to PDF device, All/Current Page/range settings and one driver copy. Escape closed the chooser and returned to the preview. The filename and device controls were visible in accessibility text but unavailable as targetable owned windows. Indexed filename input could not resolve the embedded element, keyboard focus stayed outside the editable modal controls, and modal window capture timed out. Full native file acceptance and app-driven print submission remain unverified. M7-B2A independently proved actual two-page Windows virtual print job 3. The renderer and native PDF publication focused tests independently verified file output from the retained scene. The synthetic original Note stayed at SHA256 C07195B435E4C9351DD6A7B11BFE2EED94AF5DC4A13CE1FF67B73622A75D230B and the output folder stayed empty after cancellation. All disposable launcher processes used for the smoke were stopped.


## M8-A1 secure password and passphrase generation

M7-B2B was pushed at 0be85f2cf872d8077bc8b6a183c3635c3bbb858d. The generator validates shared password and word-count bounds and samples with direct fallible Windows-backed OS randomness. Each range uses bounded rejection and passwords reject completed candidates that lack any configured class. The exact inclusion–exclusion cardinality accounts for required classes and ambiguity filtering. Passphrases count independently sampled words and enabled suffixes. The score rounds down the base-two logarithm. Fixed casing and separators add zero. Generated values are ephemeral, carry a checked typed result ID and policy revision, and redact secret text in Debug.

The embedded EFF long word list retains its original 108800 bytes and SHA256 ADDD35536511597A02FA0A9FF1E5284677B8883B83E986E43F15A3DB996B903E. All 7776 dice keys and words were validated. The generator samples the immutable alphabetic pool of 7772 entries, which allows deterministic separators without ambiguous segmentation. The four hyphenated entries remain in the unmodified licensed resource. The local notice credits Joseph Bonneau and the Electronic Frontier Foundation under CC BY 4.0. M8-B will carry this attribution in the distributed utility.

Three initial focused tests passed. Independent review improved exact class test draws and passphrase punctuation and capitalization coverage. The final run passed 4 of 4 in 0.135s after a 1m 52s compile, including direct OS generation and redacted result checks. Exact command: cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(password_generator::tests::)'. Diff whitespace verification passed. The next checkpoint adds native protected clipboard publication and capture admission before enabling any generation commands.


## M8-A2 protected clipboard publication and app read admission

M8-A1 was pushed at d51a629621001f7b490585cbc7ce8a143f56fcde. Protected Copy accepts only an owned result from the generator. The Windows boundary prepares a WinRT package containing text and a constant private format before making one SetContentWithOptions call. It disables Windows clipboard history and roaming for that item. It distinguishes a refused copy from a Windows-accepted copy whose Flush did not confirm persistence. The copy path never clears the clipboard or restores an earlier item. The reader checks the marker on one DataPackageView before requesting text, and treats classification errors as unavailable. History admission, Clipboard Modify preview and undo, macro clipboard transactions and freeze-paste, and the shared snippet, sequence and diff text capture paths use that reader. Regular OS paste and normal clipboard writers retain their existing APIs.

Independent review identified refusal wording and potentially slow delayed providers. Interactive search and shared command capture now give delayed text a bounded 32 ms asynchronous wait. Provider access occurs before acquiring the history mutex. Other clipboard text readers may wait up to 750 ms. The initial targeted run passed 16 of 16. The final run passed 16 of 16 in 0.461s after a 1m 49s compile. Exact command: cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(protected_clipboard_) | test(clipboard_modify::clipboard::tests::) | test(clipboard_none_and_failure_are_best_effort_and_success_stays_transient) | test(clipboard_action_runtime_reads_and_writes_exact_text_once) | test(paste_does_not_restore_when_snapshot_or_temporary_write_fails)'. The tests use native in-memory WinRT data packages, fault-injected refusal and flush, and disposable history files. Both a marked package and an ordinary package with identical text went through the actual classification path. Reopening the history consumer still skipped a marked item, and its following ordinary copy was admitted. Existing Clipboard Modify and affected macro behavior passed. Diff whitespace verification passed.

A background Windows desktop process obtained a native clipboard view without requesting any underlying format or content. A separate Windows native in-memory package probe verified both data formats and both options. Attempts to create a fresh isolated window station failed with Win32 183 for the logon-derived name and Win32 5 for a unique named station. This Windows Core installation has no Windows Sandbox executable. Live protected publication, preservation of existing system data on contention, and persistence across a process restart remain unverified on this host. No host clipboard text was queried or changed. M8-A3 routes the old random password commands through protected activation.


## M8-A3 activation-only legacy password commands

M8-A2 was pushed at 979b112582416a0595e5f4567c4f799f3645938b. The rand/random pw/password search outputs carry an action recipe and validated length. Search leaves ordinary number and dice RNG state untouched. The typed password:copy action generates only at activation, through the existing protected clipboard publisher. The namespace preserves user-authored clipboard literals, including literal text that resembles a password recipe. Launcher activation preserves the normal focus and query behavior, skips history, usage and favorite-run tracing, and produces fixed status text. A refused launcher copy keeps the same private result available for an explicit same-length retry. A headless refusal retains the redacted private result in a typed error for explicit retry by the caller.

Independent review found that saved macro steps resolve legacy random password queries through the headless executor and that the historical radial clipboard handoff must remain ExternalInput. Both now use the protected typed path and preserve those existing behaviors. The final fixtures exercise actual query resolution and typed headless dispatch with an injected writer, launcher no-retention outcome, failed-copy retry ID, strict malformed or attached input rejection, literal compatibility, radial policy and ordinary seeded random behavior. No host clipboard was written in verification.

Initial targeted verification passed 6 of 6 before the compatibility review. The final focused run passed 7 of 7 in 0.170s after an 8m 55s compile with cargo nextest run --offline --config-file target/productivity-toolkit-nextest.toml -P productivity --lib --test plugin_queries --cargo-profile fast-dev --locked -j 1 --no-fail-fast -E 'test(password_wire_is_strict) | test(password_headless_) | test(password_search_is_options_only) | test(legacy_password_) | test(random_plugin::generate_number)'. Diff whitespace verification passed. M8-B adds the hosted password/passphrase utility and only persists its options.


## M8-B hosted password and passphrase generator

The Random plugin exposes an exact password entry and a payload-free typed dialog command. Both modes use the retained utility host. Only generator preferences and ordinary geometry/presentation persist. The current result stays in the dialog session. Generate, Reveal and protected Copy are explicitly bound to the accepted frame, session, policy and result identity. Editing options invalidates the prior result. Host changes remask it. Reveal paints the result with a safe accessibility description, keeping plaintext out of widget metadata and the macro recorder's UI Automation name capture. The passphrase view credits the EFF word list and labels the entropy estimate.

The scoped Nextest expression was test(password_generator) | test(password_dialog_) | test(password_preferences_) | test(expanded_dialog_layer_inventory_matches_actual_window_ids). All 10 passed. The retained test exercised keyboard focus and generation, Reveal, the exact result ID at the injected Copy writer, rejection of a discarded Copy click, preference persistence and real detached-viewport registration. The AccessKit test inspected nodes without printing generated output.

The ordinary Windows build completed with SHA-256 9D86C819CAB27C93D38306D58ACA2DCACBD48012A8603B0342A56057AA4BE818. With only the Random plugin enabled in an isolated profile, the live app opened from search, generated a masked password, revealed and concealed it, invalidated that result on the mode change and generated a masked passphrase. A separate Password Generator window opened and its presentation was saved. Windows automation exposed only its frame in this session. The profile had exactly the mode and two option sets under password_generator. For the restart observation, the fixture's saved presentation alone was set back to docked. The live app loaded the saved passphrase mode with Copy disabled and no result. Both test processes were closed. Live Copy and history ingestion across restart remain unverified because the isolated profile shares the desktop clipboard and the separate window-station preflight failed in M8-A2.


## M9-A1 read-only Windows transfer admission

The new transfer domain captures an exact local parent and ancestor chain, a reviewed source entry or a destination occupant. Windows opens each validated component relative to a still-held parent and uses no-reparse/no-recall flags. Only STATUS_OBJECT_NAME_NOT_FOUND on the requested leaf proves vacancy. A fresh probe compares the exact ancestor and entry identities and captured metadata. Target observations release their read-only handles on return. The worker must later acquire its own held guards and apply stream/content checks before write or deletion. Case-insensitive ordinal comparisons preserve ordinary case-only collisions, and the Windows case-sensitive directory flag is checked for both parents and selected directory entries.

The first native run passed six checks and identified STATUS_INVALID_PARAMETER for the combination of FILE_DIRECTORY_FILE and FILE_OPEN_NO_RECALL. The corrected transfer path keeps no-recall and verifies directory type on the opened handle. The existing ZIP methods retain their prior flags. All ten focused Windows cases passed after review corrections. The filter included transfer_admission_ and the three shared c06 junction tests. These cases exercised a late occupant, a replaced parent, a real hard link and case-only collision, a real retagged junction, cancellation, cloud attribute classification, invalid paths, and exact NTSTATUS separation. Enabling case sensitivity in a separate empty test directory returned Access Denied, so the positive case-sensitive fixture was unavailable. C: and G: both report local NTFS and distinct mounted volume identifiers. This enables a later cross-volume test in disposable roots.

Sources: [Windows NtCreateFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntcreatefile), [case-sensitive directories on Windows](https://learn.microsoft.com/en-us/windows/wsl/case-sensitivity), [Windows file timestamps](https://learn.microsoft.com/en-us/windows/win32/sysinfo/file-times).


## M9-A2A bounded recursive source review

Transfer review now preserves explicitly chosen root order and enumerates each ordinary directory by a held native handle, with Windows ordinal child order and parents before children. The transfer adapter reuses the existing rename enumeration parser. Each child is observed relative to the same held directory. Directory identity, ancestor evidence and source metadata are checked around enumeration. The planner rejects overlapping roots and duplicate native identities, including hard links found deeper in the same tree. It retains unsupported entries with their reason and does not descend into them. The entry, depth, path, retained-metadata and cancellation limits refuse incomplete trees.

Directory listings preserve placeholder and reparse classifications before child admission. Windows can expose recall-on-open attributes during enumeration alone. The planner records those entries as unavailable without opening them. Ordinary child probes still use no-recall and fresh handle metadata. All 14 focused Nextest cases passed after this review correction, including a real junction, hard-link and overlap rejection, sorted snapshots, cancellation/bounds, the regression case for a listing-only cloud flag, and the directly affected native rename and archive cases. Staged whitespace verification passed.

[Windows file attributes](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)


## M9-A2B destination and conflict review

The transfer review maps Copy, Move and Duplicate in source order. Each destination refers to an exact held-parent observation or an earlier directory entry that the worker must create exclusively. It refuses source aliases and descendant destinations, and surfaces case-insensitive name collisions within the job. Duplicate makes a reviewed vacant sibling proposal. The configured copy verification choice is retained. Streamed outputs inherit destination permissions, while an atomic rename preserves source permissions.

The current review generation binds each explicit Skip, Keep Both, Replace or Merge choice. A group choice applies to an explicit compatible set of occupied rows. Merge admits the exact existing directory and exposes child conflicts independently. Keep Both checks a fresh alternate under the exact current existing or planned parent. A competing arrival that changes a proposed Duplicate parent invalidates a child alternate. A maximum-length source leaf can use an explicitly chosen shorter alternate. Before sealing, the plan checks every retained source and occupied or vacant destination observation. Pending conflicts, unsafe entries and stale evidence cannot seal. The worker will acquire held guards and check supported streams before publication or source removal.

The final scoped Nextest filter test(transfer_review_) | test(transfer_tree_) passed 16 of 16. Observed cases cover current-generation rejection, late replacement, group scope, a real case-only collision, the maximum supported NTFS name, sibling duplication, exact planned-parent binding, and independent merge conflicts. All fixtures used disposable roots and approval produced no write effects. Staged whitespace verification passed.


## M9-B1A guarded ordinary file copy

An approved vacant-file entry reopens the exact source and destination parent chains. The source handle excludes unnamed-stream writers. The destination uses an exclusive write-through stage and the existing handle-relative no-replace rename. The staging name excludes the approved final name under Windows ordinal comparison. A native failure after stage creation can identify the attempted leaf and parent for reconciliation, with a proven file identity when available. The approved verification requirement controls the destination reread, and streamed moves always require that verification. The ordinary file adapter preserves supported attributes and the creation and modification times.

The adapter checks the source and output stream inventories around copy and verification. It refuses unsupported or unknown alternate streams. A late stream change returns a failure with the still-identifiable published artifact. The next slice admits the specific Zone Identifier stream. Existing archive and PDF callers retain their publication API.

All 11 focused Nextest cases passed. The filter was test(transfer_copy_native_) | test(c06_root_relative_publication_commits_exact_bytes_without_replacement) | test(c06_stage_identity_failure_reports_cleanup_and_preserves_foreign_files) | test(pending_read_cancel_drains_native_result_and_releases_owned_resources). Disposable Windows tests exercised real multi-chunk and empty copies, exact data and timestamps, SHA-256 verification, writer exclusion, stale source and parent refusal, cancellation and stage cleanup, reserved staging names, a late foreign destination, and a late source stream. Staged whitespace verification passed.


## M9-B1B guarded Zone Identifier preservation

A bounded NTFS inventory admits the unnamed stream and an optional Zone.Identifier data stream up to 64 KiB. Unexpected, ambiguous or unsupported stream layouts stop the copy with a typed failure. The adapter copies Zone Identifier separately and verifies its digest in both ordinary and requested full-verification modes. The exclusive staged stream stays held from write through no-clobber rename and the final readback. The copy reports unnamed bytes and Zone bytes separately.

Source streams are checked at final namespace admission and after publication. A change observable before rename prevents publication. A change first observed afterward produces an identifiable published-artifact failure. The native fixture attempted a write to the exact staged Zone stream and Windows denied it with a sharing violation. The same fixture proved both streams and creation/modified times after release in both verification modes.

All 14 focused native-copy and shared archive checks passed with the M9-B1A filter. The added cases include actual NTFS Zone bytes, a retained stream guard, bounded malformed record rejection and late-source changes on each side of publication. Other named streams remain explicit unsupported outcomes. Staged whitespace verification passed.

[Windows file streams](https://learn.microsoft.com/en-us/windows/win32/fileio/file-streams) · [Windows stream records](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_stream_info)


M9-B2A verified on Windows with 21 targeted Nextest passes. Copy and Duplicate create guarded directories and apply directory timestamps after completing children. Explicit Merge preserves unrelated contents and reports late source changes. Native fixtures verified late competing directories, replaced parents, named stream refusal, definite no-effect preflight, and existing archive destination behavior.


M9-B2B passed 40 focused Windows Nextest tests after independent review. A single app-owned worker serves an explicit Start batch in visible order, bounded to 32 queued and active approvals, and projects actual pause, cancellation, progress and retirement to Activity. Native fixtures proved safe pause before cancellation, exact queue and Activity identity, directory finalization and late-cancel preservation of a published file. Guarded Replace, Move, durable receipts and the user-facing center belong to subsequent checkpoints.


M9-B3A passed 37 focused Windows Nextest tests after the native review and NTFS timestamp fix. The private journal-required Replace path retains the exact old object, preserves a late foreign winner, restores the incumbent's original spelling on safe cancellation and stops later writes when a receipt fails. Fixed, distinct creation times verified that the exact held published object preserves the approved copy times even with NTFS filename tunneling. An injected rename-result ambiguity after a real rename retained the published bytes and file ID. Ordinary queue activation remains deferred to durable receipts in M9-C.


M9-B3B passed 44 focused Windows Nextest tests after independent review. Native same-volume Move to an approved vacant name retains the file ID and streams, verifies the currently named Zone Identifier when requested, and keeps an exact relocation receipt after a late journal failure. A late competitor blocks admission, unknown streams remain intact on unverified atomic rename, and explicit verification refuses unsupported streams. Real attribute-only edits after Move rename and Replace publication survive guarded creation-time repair. Normal queue activation remains deferred to the durable recorder.


M9-B3C passed 50 focused Nextest tests after independent review, including six opt-in real C: and G: disposable fixtures with distinct native volume identities. A 192 KiB file and its Zone Identifier were published, verified, and retained while the exact guarded source was relocated and removed. Absence was confirmed after all source handles closed. Cancellation, a refused verification receipt, and changed destination metadata preserved the source. A foreign original-name arrival survived with the former source recoverable by its exact identity. These fixtures use an in-memory receipt adapter. The app still requires the durable recorder before enabling the path.


M9-B3D1 passed 54 targeted Windows Nextest checks after independent review. Move with explicitly approved Replace uses the common verified publication and guarded source retirement on one volume and actual C: and G:. Native tests verify the displaced destination's exact ID and alternate stream, the moved output's Zone Identifier, and preservation of all three objects when retirement intent is refused. An attempted deletion of the still-held incumbent was denied. A concurrent attribute-only edit to that backup stopped source retirement and was preserved. The public queue still awaits durable journal activation.


M9-B3D2 passed 62 focused Windows Nextest checks after independent review. The worker resolves planned destinations through the exact created parent. Native nested Move succeeded on one volume and across actual C: and G:, including source cleanup in child-before-parent order, directory timestamps and Zone bytes. A reviewed skip, late source child, replacement source folder or late target metadata edit preserved the appropriate original. The kernel's nonrecursive disposition independently rejected a nonempty guarded directory. The journal-required worker paths remain private until the durable store is wired.


### M9-C1A receipt framing

Six focused Nextest checks passed (run `5862a6bf-494e-44f6-ad15-0cc2008adece`). Create-new, ordered SHA-256 frames, bounded read and write capacity, write-through plus sync, and Windows read-only sharing protect the active receipt. Readers preserve the valid prefix at the first malformed, truncated or over-budget frame. This checkpoint supplies the format for the upcoming durable store.


### M9-C1B1 typed receipt evidence and held native files

Twelve targeted Nextest checks passed after independent review and remediation (run `cf2e3755-7905-4136-bfb3-50785d1290bc`). The typed wire validates exact source and runtime destination, effect lane and ordering, verified staged source retirement, guarded rollback, directory creation-time evolution and terminal outcomes. The native bridge uses synchronous held relative files, create-new write-through, no-follow/no-recall, guarded same-handle pruning and bounded frames. Exact applied-engine receipts and restart reconciliation still require the store and runner checkpoints.


### M9-C1B2 durable receipt storage

The store is read-only until explicit admission and pins the exact physical profile and receipt chain. Admission charges verified physical file lengths, limits decoded metadata and reserves bounded effect, compensation and terminal space. Corrupt and oversized receipts stay visible and charged. Pressure can remove only exact clean settled files on the same read-and-dispose handle after revalidation. The union passed 18 checks (`308744aa-bf01-46cc-be45-e9847e7d506e`). A final six store checks passed after also refusing a missing previously pinned root (`337ff0af-2c04-46ea-a774-b248a0e4a190`). Applied engine and worker activation follow.


### M9-C1B3 applied receipts and transfer worker

The app-owned worker opens its profile receipt store after an explicit Start and durably records the applied engine effects for ordinary Copy/Duplicate, directory finalization, verified Move and reviewed Replace. The 87-test Windows union passed (`7e54ff8c-4255-4347-b101-838767ca03fa`), with actual distinct-volume regressions, a fully finished case-differing Merge, staged source restoration after its disposal intent, retained exact Replace backup and live Copy reopened from disk. The final 14 checks passed (`ff7fe291-58a3-40f6-a384-7996f76c69c0`) after adding actual worker Activity Partial and Confirmed retirement with a sanitized diagnostic on terminal receipt refusal, and truthful cancelled directory metadata rows. Queued-only durability, interruption reconciliation, Retry and eligible Undo follow; cleanup without a durable exact observation remains conservative.


### M9-C2A read-only restart reconciliation

The receipt history reports Interrupted from a clean unfinished prefix and retains valid evidence before a corrupt tail. Selected details use an exact held receipt and recheck recorded artifact identities, revisions and patched parent creation stamps, with no replay. A generated name without assigned identity never confers ownership. Inaccessible and unsafe entries stay visible without hiding other jobs. Verified atomic Move reversal evidence requires clean completed rows, historical and current original vacancy, and unchanged output together with both historical parent proofs and the stored digest. The 19-test worker/store/recovery union passed (`d100a10a-70c5-4987-8b75-732070d275bd`). The final 12 recovery and store checks passed (`9e95d2e6-010d-4cd5-bde7-20eae80b9704`) including real case-renamed and unsafe sibling receipt fixtures. User-facing Retry, Undo and unstarted queue persistence follow.


### M9-C2B explicit Retry and verified atomic Move Undo

Retry prepares a fresh single-file review of an unchanged, untouched row with its exact reviewed parent and leaf. Confirmed nested parent evidence is required. Completed/possibly effected rows, occupied targets and a catalog that could conceal prior Retry are refused. Related receipts record the original job/item and guard actual repeat admission after external output removal. Undo is explicit and restricted to clean completed verified same-volume atomic Move with both historical parent proofs and exact vacancy. It rechecks the held file and current named Zone Identifier digest before rename and uses the guarded no-replace rename. A durable Undo origin refuses generic Retry after interruption. The 26-test union passed (`3c8e5de2-7868-40fc-a828-956207b868b6`) and the final four action checks passed (`747fe525-2d68-4fe3-8552-66c9e608199d`), covering actual nested retry, completed-repeat refusal, named Zone changes, late foreign original, interrupted Undo binding, and successful same-ID restoration of both streams. History/queue service and user-facing actions follow.


### M9-C2C1 ordered unstarted queue snapshots

The pinned receipt namespace now stores display-only queue generations under exact create-new handles. Generations preserve a prior complete snapshot and retain torn files. Completion of a new generation survives failed cleanup of an older one. A 512 KiB and 64-entry reserve remains available inside the aggregate receipt limits. Held metadata accounting permits queue saves while an actual transfer receipt is open, with short shared coordination at initial admission. The final 15 targeted queue/store/recovery-action checks passed (`a569166a-b00c-4980-8d78-9ac391171ed0`). The worker integration and restart summary requests follow.


### M9-C2C2 durable queue and recovery service

Explicit Queue writes an ordered non-authorizing snapshot before acknowledgement. Accepted jobs keep the same durable identity through applied receipt admission and snapshot retirement before the first file effect. Restart previews appear as Interrupted and are never rearmed. Exact queue edits persist while a Copy is active. An uncertain retirement keeps later armed work untouched, with de-duplication on a positively observed applied receipt only. History, details and Retry/Undo preparation use bounded requests on the existing worker, so requests wait during active or paused transfers. The 34 scoped checks passed (`6b1013bc-b078-44c1-9669-2333435c0cd9`), including restart order/discard, stable handoff, real active edits, uncertainty fencing and prior native receipt cases. Three redundant mut bindings identified by that gate were removed. M9-D exposes the service in the launcher.


### M9-D1 hosted File Operations Center

The retained Windows utility captures exact picker, review, queue and recovery intents only after an accepted host frame. Review and durable queue edits use background work outside the frame callback. Draft revisions and source-tree identity fence stale pickers, scans and review selections. The center displays actual live job phases, sources, progress, ordered controls and paginated outcomes with retained paths, plus receipt details, interrupted previews and confirmed recovery queue actions. The 14-check worker/controller union passed (`eabc27d7-252a-484f-a87d-fdf9190b224d`). The final three controller/view checks passed (`48fce880-4534-423f-b353-46699fc942dc`), proving missing picker owner refusal, stale draft handling, and actual Review/Queue/Start separation in a disposable fixture. Launcher/File Search/Activity entry points and native-window smoke follow.


### M9-D2 launcher, File Search and Activity entry points

The Windows `fileops` command opens the existing center through the typed GUI command bus. File Search passes a single exact current row through its retained parent action. Activity resolves the full key only after a retained root frame, focuses the associated live/queued/completed job, and reports a job that has aged out while keeping the Activity window open. The final 11 focused checks passed (`88026e16-175c-407f-8a5e-096611b4dca2`), including exact current File Search handoff and aged-out Activity behavior. The ordinary fast-dev executable built before the Activity review repair, then the repaired Windows application code compiled in the final scoped test run.

Native smoke used an isolated profile with both Paste Sequences shortcuts disabled. The real launcher returned and activated the File Operations command, displayed the draft and opened the Windows file picker from the docked panel using the keyboard. The first cancel returned to the center. Native path entry remained unverified because the owned chooser's edit control was unavailable to the automation, and restarted-window foreground selection failed. The exact disposable source hash stayed `46D8621A473CE45AAF4757BD7B14E60812D12E67CE3808450329D34561459B03` and the destination remained empty. Native acceptance of a picker, real copy through the window and Activity click-through remain for the bounded later smoke. The M9 worker tests already verify actual Copy/Move and the hosted controller separately verifies Review, Queue and Start.


### M10-A Downloads observations and reviewed metadata

The current Windows Downloads known-folder resolver honors redirection, retrieves the path without creating the directory and frees Shell memory on either HRESULT. A saved explicit root is retained even when unavailable. A held local root yields only direct exact native entries, skips listed cloud/reparse items without opening them, and reports unavailable or changed roots and partial, limited or cancelled observations. The direct-item ceiling is 10,000 plus a name-storage budget. Existing Rename and Transfer adapters continue to reject incomplete enumerations.

The scan captures native Created and Modified fields, type, size, conventional unfinished names, older installers and same-size suspected peers. At most 512 guarded Zone probes read up to 16 KiB each and 512 KiB total; valid Internet/Restricted attachment metadata can show a web host. Other origins are Unknown. The profile store atomically saves the selected root, destination preference and at most 10,000 review marks. An exact leaf, ancestor identities and creation stamps, item identity and full observed revision bind each mark. Read-only open leaves an absent store absent and retains malformed data for diagnosis.

The initial six native enumeration checks passed (`697e6fbd-a3a2-4266-ab6b-e90d2be21810`). The final 12 checks passed (`699a639a-3acb-4370-8d2a-009c40f2535b`), including real disposable Zone.Identifier input, shallow native cap, strict wrapper parity, explicit missing root, durable review reopening and rejection after edit/replacement. The complete slice received independent source review. No user's Downloads directory was scanned for verification. M10-B will wire the view and explicit actions.


### M10-B hosted Downloads Triage and selected actions

The Windows `downloads` command opens one dockable utility. Open or explicit Refresh alone scans the proposed root. The view shows partial and skipped observations, Created and Modified metadata, search, independent sort and grouping, review states, and incomplete, large, installer, archive and duplicate views. Choosing a different root or saved destination is explicit. The runtime publishes scan generations and rejects stale or broadened action selections. Hidden work retires without an idle repaint loop.

Open and Reveal use the current guarded item. Rename uses a held no-replace file or directory operation. Compare independently verifies the two selected regular primary streams and earns only the main-data duplicate label. Copy/Move reuse File Operations with exact reviewed native roots admitted before descendant enumeration and a separate Review, Queue and Start. ZIP intake compares held source and parent evidence before catalog reads, and extraction retains the existing Preview and Confirm. Only completed writer output with rechecked archive revision can record full or selected physical coverage. The profile store retains both count and byte bounds and uses a distinct warning if the output succeeds and history could not be saved.

Recycle requires confirmation of the exact entries, the owning window, and the Shell recycle-only and no-connected-elements flags. The adapter rechecks the item immediately in the Shell pre-delete callback and requires returned Recycle Bin evidence for success. The supported Shell path/PIDL handoff has no documented atomic identity fence between its callback and deletion. The adapter keeps native Restore semantics and reports uncertain or partial outcomes.

The 30 targeted tests passed (`e07a5af5-2716-412a-b147-750bae8cefd0`). The final six checks passed (`a698d911-3780-4f8a-a0b8-5c4da3b6679f`), including the byte-cap continuation and an actual exact HTML fixture sent to the Windows Recycle Bin, with its unselected companion folder preserved. A separately cancelled request attempted zero items. The ordinary fast-dev desktop executable built. It opened in an isolated fixture profile with both Paste Sequences shortcuts disabled. Native automation reached the launcher query but the foreground process check prevented command entry, so the reviewed-to-move, ZIP, Reveal and Recycle-confirmation window sequence remains unverified. The four fixture files remained present and the destination remained empty. No user's Downloads directory was read or used.


### M11-A Inbox records, retention and local actions

The profile-owned Inbox stores typed source and event identity, timestamps, severity and status, read and pinned state, local history changes, and current related-item receipts. Internal text comes from a fixed allowlist. Opt-in Windows records keep OS identity and content generation separate. Default retention is seven days and at most 1,000 entries including pins. Capacity drops are disclosed and bounded suppression prevents replay of cleared or retired facts. Source mute and exclusion are separate. External capture, external text, and across-session external persistence have separate controls and default off. Turning off text removes retained external text. The atomic bounded projection excludes Windows rows and their local suppression history until durable retention is explicitly enabled. The notification store is excluded from default data backups.

Exact local action tickets bind the current fact, status and source generation. A typed Create Todo request uses the existing Todo transaction and a deterministic fact-derived ID so an explicit retry returns the same task. Wrong or stale tickets fail without running text from any source. Calendar and Timer related owner adapters follow in M11-B; external installation and native capture remain in M11-C/D.

The initial 10 scoped checks passed (`46ce3a5b-f99b-49ae-ade6-c930c8834f74`). Independent review prompted generation freshness across clock rollback, same-time action invalidation, and count-eviction disclosure. The final 12 checks passed (`f3aae19e-13ca-4936-9f48-676f303f4570`), including typed durability, corrupt/failed write preservation, isolated session payload exclusion, backup policy and exactly one stored Todo after retry.


### M11-B Internal producers and hosted Notification Inbox

The ordinary launcher opens the same retained Inbox from either `inbox` or `notifications`. It provides searchable and grouped history, read/date/source/category/app filters, pin, exact related navigation, source and app mute or exclusion, explicit local clear, and actionable capacity and write feedback. Muting suppresses the unread indicator while retaining browsable records. Dock and pop out use the same retained owner and accepted action pass.

A bounded profile worker adopts sanitized launcher, Timer, Calendar, Macro Notify, Hot Folder and Activity/transfer facts. Transfer job IDs include the launcher session so restarting cannot merge prior or current job numbers. Calendar reads the same profile events, publishes due reminders once after durable Inbox adoption, and supports bounded current-occurrence snoozes without editing the event. Timer snooze uses the existing owner. Related actions verify exact owner state. Shutdown retires owned producers before draining admitted Inbox work.

The scoped Nextest run passed 34 checks (baaea902-7b62-45fa-8fdb-3fb550c58fb0). The expanded run passed 35 checks (including an actual Windows transfer copy and its single sanitized receipt). Its new GUI fixture initially supplied raw queries to the typed parser. Using the production query-to-plugin-action path, the exact follow-up test passed (7b57aa77-10a5-4c25-9c93-8ae1e7839b78). Independent review findings on session identity, shutdown order, Calendar profile identity, hidden-root navigation, mute and capacity disclosure were addressed.

The ordinary fast-dev Windows build succeeded. In isolated profile `m11-b-native-9b475f2f` with both Paste Sequences shortcuts disabled and external capture off, the real Timer scheduler fired a synthetic alarm. Keyboard activation opened the docked Inbox, found one unread Timer event, selected it, and created exactly one Todo with visible confirmation. The retained event omitted the alarm label. The keyboard Pop Out command opened the native Inbox and persisted its preference. Native window accessibility exposed only its frame and Windows Graphics Capture timed out, so popped-out contents and the transfer-to-Inbox GUI sequence remain unverified. A real worker transfer and exact typed Inbox record were verified in the focused test.


### M11-C Optional Windows identity and foreground consent

The Inbox defaults to Off and initializes no WinRT access service while disabled. Explicit opt-in can reveal Setup required for the ordinary executable. The adapter checks the running package family and exact application user model ID. A retained request from the active foreground launcher thread invokes Windows consent asynchronously. The outcome and later permission checks remain distinct from capture status. Title and body, cross-session retention and collection have separate controls.

The optional Windows SDK script copies the ordinary executable, retains an existing Win32 manifest when present, embeds the matching sparse identity, and packages the capability. Deployment and exact removal are documented separately. The script does not install the package, purchase signing or change trust. SDK version 10.0.26100.0 produced the expected unsigned package (`4FED825DBA98C4BCDE41250F1C09E6BFD2E8997A8D120502540CC40DC5F20E0C`) from the checked-in script with an unchanged ordinary executable. A separately staged native foreground probe verified its embedded manifest bytes. A disposable external synthetic sender is compiled for M11-D. Neither probe nor sender has been installed, trusted or run.

Four scoped automated checks passed (cb62ed72-c818-4594-9992-b5c8c7920ab8), including unpackaged aliases, disabled WinRT behavior, exact identity validation and real internal transfer operation. Independent review found no actionable source issues. M11-C remains in progress because actual signed-package Win32 consent still needs authorized temporary certificate trust and native observation. Microsoft documents that some WinRT consent methods are unavailable in desktop processes. The unsigned package and tests do not prove that this listener's permission works on this host.


### M11-D Windows listener and exact current-item actions

The existing Inbox worker owns the optional notification subscription and bounded current-toast reads. Permission is checked before reads, during large batches and before adopting results. Change callbacks coalesce work, a bounded periodic refresh checks permission and missed changes, and unchanged checks do not repaint. Coverage reports Off, Setup required, Permission not granted, Denied, Active, Disconnected, Unsupported or Error and states when collection began. Limited snapshots never imply source removal. The adapter filters its own app identities and bounds each observation to 512 items and 1 MiB of normalized input. The authorized debug signing identity additionally filters to the one synthetic sender before retrieving notification text.

The local projection keeps read/pin metadata across source content updates and distinguishes reused Windows IDs by creation time. Source actions recheck current app, notification ID, creation time and Windows permission. Dismissal with text enabled also rechecks current title and body. The native API only accepts a numeric ID for the final call, so Windows provides no atomic identity-and-version fence. The UI reports a dismissal request separately from a subsequent observed removal. Opening a supported source validates an installed package identity through Windows and supplies no launch arguments.

The 37-test Inbox/backup run passed (d3975fba-a926-49b8-be06-393ba4a17a7d). Review identified the in-place source update window. After adding current text checks and removing redundant idle repainting and GUI permission polling, 14 scoped checks passed (b8ee9b28-21eb-4207-b53a-232ee7013e41). These included stale/missing dismissal text, ID reuse, incomplete snapshots, bounded UTF-16 handling, unpublished idle state and ordinary internal operations.

After explicit approval for Current User trust, the SDK signed the exact short-lived non-exportable fixture package (SHA-256 `593C80368142F6A7D8BB0B3B11D2CBA8B30AFDEF687FFF5AC6502FFA64C495AB`). Windows 10 build 19045 `Add-AppxPackage` rejected it with `0x80073CF0` and inner signature error `0x800B0109` despite the exact public certificate in Current User Trusted People. The import was rolled back and no fixture package is installed. The in-progress C/D native acceptance requires a signed installation with trust recognized by Windows, actual permission and the independent synthetic toast. Temporary Local Computer Trusted People authorization is a separate security decision.


### M12-A1 File Search Preview and Hash keyboard ownership

The existing retained holders now publish distinct current Preview and Hash owners in docked and native presentations. Targets bind the current preview request or hash draft generation. Preview registers its header, close and presentation controls and a focusable text target. Owned Page Up/Down, Home/End and arrow keys scroll its existing virtualized text area and preserve pending matched-line reveal. An unowned preview leaves keys available to the current owner. Hash registers its fields, current actions and owned source/algorithm choices. Hint navigation alone focuses action controls and never starts hashing or copies output.

Six initial focused checks passed (49971aec-1802-4a41-ab65-9e66cf37f14e). Independent review identified the need to navigate long preview contents. The four final checks passed after the review correction (afc8a934-c67d-47cc-8a12-94c01e791bbd), covering long-preview focus and scrolling, scaled rows, the actual layer inventory and a retained Hash hint that focuses Calculate without running it. Preview scrolling was exercised at the shared body boundary. Native Hash popup and Preview scrolling interactions remain for representative final smoke.

### M12 native Downloads, File Operations and Inbox handoff

In isolated Windows profile `m12-native-transfer-3050f698`, both Paste Sequences shortcuts were disabled before startup. Keyboard activation opened Downloads, selected and marked one synthetic file reviewed, and opened the prefilled Move draft using the stored synthetic destination. Review showed the exact one ready row. Queue left the source and destination untouched until explicit Start. The native transfer completed a 57-byte same-volume Move. The source was gone and the destination retained SHA-256 `02D034B6CB9AFF5F4277D6B4F9AD95C2FFFC883EEDB276ACE72039730AD1C881`, matching the recorded input. The native Inbox showed one completed transfer without retaining the path. Its Open related action and Activity's Open owner tool each focused the exact retained completed job in File Operations. Escape closed the File Operations child while leaving Downloads visible.

This closes the earlier M9-D2, M10-B and M11-B representative native Move and related-navigation gaps. The original direct file picker acceptance remains unverified. A separate synthetic ZIP was staged, but foreground PID checks failed before extraction could be started after relaunch. Its archive remained present with no extracted output. Both owned launcher fixture processes exited. No user's Downloads directory was used.


### M11-C/D final native deployment attempt

An unregistered executable with valid embedded fixture metadata was launched without setup. The actual Windows process APIs returned `0x3d54` for no package and `0x3d57` for no application identity. The probe exited before requesting consent. This is native evidence for the unavailable identity path.

The user separately authorized the same exact temporary public certificate in Local Computer Trusted People after Current User deployment returned `0x800B0109`. The normal Windows elevation request returned “The operation was canceled by the user” before the elevated helper started. No machine certificate was imported. Exact cleanup verified that the fixture package, Current User trust, Local Computer trust and the disposable Personal signer and its key were all absent. The synthetic sender was never registered and no test toast was emitted. The optional listener and its focused tests are implemented. Actual packaged permission, an external toast entering the Inbox, and native revoke behavior remain blocked in this environment. [Microsoft MSIX certificate troubleshooting](https://learn.microsoft.com/en-us/windows/msix/msix-troubleshooting-guide)


### M12-A2 QR and Explorer Tabs keyboard ownership

QR and Explorer Tabs now register their real docked and native owners. QR exposes the source editor, inline error-correction choices, advanced controls and available exports through retained targets. Layout replay keeps interactive responses registered while accepted first-pass ownership still controls input effects. Explorer uses stable saved-list IDs and current picker snapshot identities, including after reordering or refresh. Its active saved-folder picker owns Escape before the outer host. Keyboard traversal moves among the filtered original snapshot indices and reveals the current row in the virtualized list. Search and editor focus retain their own input.

The first six scoped checks passed (d5200eb1-8db5-4cec-a804-096e479fa1d4). Independent review led to the replay, stable identity, Escape and paging corrections. The resulting run passed five checks, including the forced-discard QR target/effect fixture and filtered picker keyboard traversal (72d0ae26-fe58-4bf8-a1e2-ee39c25509e2). The older picker fixture still expected a retired incarnation to select an entry after refresh. It now verifies both retired rejection and current selection, and the exact rerun passed (12ff82a1-6834-45c0-b454-685fd4365bb1). Direct native picker navigation and QR effects were outside this focused gate.


### M12-A3 File Lock and Folder Size keyboard ownership

File Lock and Folder Size keep their existing retained docked and native hosts. Their current form, result and action targets now participate in Jump. Process rows bind the inspection generation, PID and creation stamp. Folder rows bind the retained projection generation and node identity, with owned context menus excluding parent targets. Keyboard row traversal stages current selection and one-shot reveal. Accepted mouse selection transfers focus to the exact selected folder row without changing its scroll offset. **Open in Folder Size** is available for a current selected directory and invokes the existing guarded drill only after activation.

The new docked focus-only cases and layer inventory passed in the initial run (7e5541b8-bf57-43cf-bc4f-de384c41b577). Three older app fixtures overflowed their default Rust test-thread stack. The new native navigation coordinators were moved out of line. A command-local `RUST_MIN_STACK=16777216` then passed the three unchanged fixtures (e2183eab-9b1b-497e-86c2-cef6009c9bbb). The final seven scoped checks passed with that test-thread setting (285c0882-eee9-4665-bc32-21ec3beefccf), including exact focused file-to-directory Arrow navigation, focus-only Drill target selection, explicit Enter and preserved native-host lifecycle. The test stack setting was applied only to the verifying command.


### M12-A4a Convert keyboard ownership

The existing root Convert window now registers its value, choices, searches, favorites, target rows, actions and preferences under its own current owner. Owned ComboBoxes register options on their corresponding popup surfaces. Stable value IDs keep choices attached to their unit. Changes in favorites, filtered membership or target rows retire the corresponding generation. Registration survives a discarded first pass while clipboard and preference effects still require accepted explicit activation. Existing Tab and focused scrolling behavior is preserved.

Adding the 33rd dialog exposed the standard array Default limit. The shared Jump registry now initializes its fixed kind-indexed slots through the complete dialog inventory, preserving each existing slot's identity. The corrected scoped run passed 21 checks including existing Convert behavior, root replay and the actual layer inventory (078cd68a-b96d-401e-9772-b124b39cba19). Its new popup fixture initially targeted the last, clipped category. The visible Area option was then used to verify that the popup alone supplies targets and that the chosen hint moves focus without changing the category. The exact rerun passed (67213882-3644-4063-8d2d-cb6b6d77a8d8).


### M12-A4b Date/Time Studio keyboard navigation

The retained Date/Time Studio registers current docked and native targets, including owned date and option popups. Keyboard hints focus controls before a separate activation. Independent review found no actionable issues. Scoped Nextest passed 5/5 in run `c20b432b-4225-46c6-8ed2-7afb253fc569`. The native popup hint interaction still needs live verification.


### M12-A5a Calendar navigation

The retained Calendar host coordinates docked and native Jump scopes. The main toolbar, exact dates, occurrence rows and the Day/Week popup register current controls. Independent review found that date-grid keys could act on the last selected day after focus moved. The corrected path uses the focused day for the next arrow or Enter. A hint leaves the selection unchanged. The focused Nextest rerun passed 4/4 (`1fcc3e2d-9225-4070-82e1-f6c9c1b2f4b2`) after correcting that finding and a borrow-check error. This covers the real root grid and toolbar, shared layer inventory, and existing detached edit and open behavior. Calendar Details and the Event Editor follow in M12-A5b. Native popup hints were not exercised in this gate.


### M12-A5b Calendar Event Details and Event Editor navigation

Both surfaces now use separate docked and native Jump scopes and register current popup choices. Calendar actions retain their exact accepted owner and occurrence checks. A Save hint focuses the current button without validating or saving the draft. The initial focused gate passed 7/7 (`6a47eb23-68f8-494e-b6fd-b387dadecd12`). Review identified clipped focus in short windows. All three Calendar surfaces now share one-shot focused-control reveal. The corrected gate passed 5/5 (`c5b9123a-74e7-46ec-9d4d-a89b4ee921d4`), including actual Tab reveal of a clipped recurrence checkbox without toggling it. Direct native popup hints remain unverified.


### M12-A6a Hot Folder Manager navigation

The existing root manager exposes current rule, editor, action picker, history and exclusive decision targets. Trigger and sample-event choices use their exact owned popup anchors. The retained root pass owns draft adoption and submission of the existing typed actions. A discarded pass preserves the live draft and sample receiver. File and folder choices run through the verified launcher window and recheck the current editor revision on return. Rule setup, explicit enable and other worker semantics remain with the Hot Folder runtime.

Review identified the ordinary ComboBox ownership conflict and clipped editor footer. The corrected gate passed 5/5 (`a1e15c58-eec3-421b-bd0f-d3c3a214822d`), including exact popup/path-ticket checks, real short-window Tab reveal, the prior read-only sample test and the live root decision that rejects discarded keyboard activation. Native chooser acceptance was not exercised.


### M12-A6b1 Data & Recovery parent navigation

The Data & Recovery window exposes its current section, snapshot, store, backup and diagnostics controls. The retained root pass owns section changes and service or shell actions. Recovery preparation rechecks current busy state and exact target and snapshot eligibility. The initial hint and a discarded Enter leave the current section unchanged. A fresh retained Enter changes it. The scoped gate passed 5/5 (`ae4d5c50-d771-427f-9b6a-76bb09a7f82b`). Independent review reported no actionable parent findings. The existing confirmation behavior remains while the exact confirmation owner is added in M12-A6b2.


### M12-A6b2 Exact utility confirmations and innermost Escape

Data & Recovery binds its confirmation to the current shared modal opening, parent epoch and typed pending intent. The decision has its own Jump owner and is applied only after its exact root pass is retained. Replacing it with another confirmation retires recovery authority while preserving the replacement. Review exposed the launcher's earlier Escape preflight. The corrected route sends Escape to the exact Recovery child with Jump enabled or disabled and to the current Hot Folder inline decision before either parent can close. A hint still focuses without applying a decision.

The initial child gate passed 5/5 (`37d4f94e-5410-46a0-8800-f13fc0ab49fa`), the Recovery Escape rerun passed 2/2 (`9cc3c86a-2669-42ce-861c-24b2e121f90a`), and the final shared Escape check passed 2/2 (`9d0e0ef8-363d-4eb2-aa1c-576b441f3114`). This closes the declared ordinary utilities in M12-A. Editor and companion coverage continues in M12-B.


### M12-B1a1 Note Templates and its delete confirmation

The retained Template Manager exposes current names, editor fields and actions in both presentations. Delete Template? uses an independent child scope keyed to the current template and manager session. Hints focus controls without deleting. Review found default-off ordinary input was tied to Jump and the native child scope used the wrong surface. The corrected docked child accepts focus without enabling its destructive action prematurely. Tests open the child through the actual Delete row for a synthetic template and verify pointer Cancel with Jump off, focus-only Delete and child Escape with Jump on, and the separate native retained target registry. The final focused rerun passed 3/3 (`e839a501-1a82-4597-b836-c7f32cf6b6b4`). Existing host reuse and layer inventory passed in the earlier focused runs.


### M12-B1a2 Unused Note Assets navigation

Current checkbox names and the retained inventory generation identify docked and native hint targets, including the sticky footer. Ordinary input remains available with Jump off. Hints focus without toggling or deleting. Existing selection, exact inventory and accepted-delete checks remain authoritative. The first focused run passed 4/4 (`f63635a7-88b8-401a-811f-22d6ceeae5e2`). Review identified offscreen keyboard focus. The correction uses one-shot reveal only when a newly focused checkbox is clipped. The focused rerun passed 2/2 (`befbe842-4e59-411a-980f-30e4427f52c0`), including a many-row fixture in short docked and pop-out bodies.


### M12-B1b Image Viewer session ownership

The actual foreground image host and its session determine root Jump targets. The native binding checks the same session's exact viewport and layer. Presentation, Zoom, Open in Default Viewer and Close participate in navigation. Image canvas gestures keep their prior handling. Independent review found no actionable issues. The first gate passed the existing retained-native and layer inventory tests. The new two-image fixture initially clicked before egui had applied the queued foreground raise. It now checks the actual raised layer and current registry before and after switching. The exact rerun passed 1/1 (`10b596ac-4921-4b84-8b47-5d08abd006cf`), with no external viewer activation from a hint.


### M12-B2a1 Dependency Explorer navigation

Current document, incarnation, evaluation basis and LineId bind the docked and native scopes for source rows and relationship links. Presentation, the source chooser, Go to source, Close and both direct/transitive relationship section headers register ordinary controls. Hint focus leaves the Pad and selected line unchanged, and a fresh activation takes the established navigation path. The first focused gate passed 4/4 (`113f0eb9-8126-4a3a-a861-e9631fa14a84`). Review found clipped ordinary keyboard focus and missing relationship headers. The corrected gate passed 3/3 (`47196171-40e0-4a96-ad64-fe4d65ac5768`), including Tab reveal in short docked and pop-out bodies.
