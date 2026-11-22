# Table Utility and CSV Viewer

G16 owns `table-utility`. Exact `table` and `csv` emit `table:open` and `csv:open`, selecting distinct supported profiles in the same retained dialog. `TableUtilitySession` and `multi_launcher_table_core` own bounded parsing, projection and operation authority. The dialog owns the selected profile, applied presentation and the current editor/selection.

| Criterion | Source-backed behavior |
| --- | --- |
| TABLE-UTIL-01 | Preserve Table auto delimiter, lenient rows and numeric policy. CSV uses strict CSV/TSV, header and lexical policy. Preserve typed UTF-8 source, explicit Paste/Open and Parse/Refresh, delimiter and row-width options and bounded limits. Opening and closing have no implicit clipboard effect. |
| TABLE-UTIL-02 | Render the current sorted/filtered virtual table with stable row and column selection, diagnostic and status feedback. Large file reading is bounded and read-only, with current result pages and status. Projection and selection actions carry current source/view identity. |
| TABLE-UTIL-03 | Copy current selection as supported CSV/TSV. JSON conversion/export requires matching current shape/header/projection. Preserve explicit previous valid output eligibility. Save JSON uses the current owner, exact reviewed payload and Replace JSON file? decision. Cancel leaves destination unchanged. Changed target requires current review. Opened input stays protected from replacement. |
| TABLE-UTIL-04 | Preserve remembered Dock/Pop Out and geometry, exact owner/pass, and lifetime/source/view/selection/export receipts. Close retires queued effects while keeping the in-process held draft/result according to the existing policy. |

Select `csv_command_is_typed_and_reserved_routes_fail_closed`, `table_dialog_open_and_close_do_not_read_clipboard_implicitly`, `csv_workflow_opens_sorts_filters_selects_and_copies_the_current_profile`, `table_dialog_overwrite_buttons_revalidate_the_captured_export`, `table_session_draft_edits_revoke_queued_export_receipts` and `table_utility_moves_the_same_held_file_page_and_selection_between_hosts`. `G16-EGUI-CSV-DATES` verifies strict CSV launcher, explicit parse, current filter and exact output-selection count. Other profile, file I/O, output, host and GPUI remain pending.
