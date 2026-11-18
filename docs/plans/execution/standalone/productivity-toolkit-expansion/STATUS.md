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
| M9-A2B, M9-B, M9-C, M9-D | pending |
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
