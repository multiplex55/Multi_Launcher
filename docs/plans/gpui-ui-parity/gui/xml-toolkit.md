# XML Toolkit

G14 owns `xml-toolkit`. Bare `xml` emits `xml:open`. `XmlToolkitDialogState` owns Source, Tree, XPath, Validation and Output and their exact native owner. The bounded XML engine/session own parsing, jobs, validation and derived output.

| Criterion | Source-backed behavior |
| --- | --- |
| XML-TOOL-01 | Open or focus the retained Source with no implicit clipboard read. Support typed text, explicit protected Paste, Copy Source, owned Open XML and Clear. Parse bounded local UTF-8, accept supported input BOM and disallow DTD/external resource resolution. Expired load cannot replace a changed source or retired owner. |
| XML-TOOL-02 | Tree presents bounded current nodes/details, expand/select and explicit source, fragment and value copy. XPath supports current expression and namespace mappings, scalar/node/empty/error states, navigation/reveal and exact selected result copy. |
| XML-TOOL-03 | Owned Choose XSD, Clear and Validate use the bounded local root-confined schema policy and exact current source/schema revisions. Suppress stale diagnostics and bound displayed errors. |
| XML-TOOL-04 | Format and Minify produce a separate bounded preview. Keep exact full output for Copy or Save. Apply Output is explicit and enforces the source limit. Save Source As and Save Output As use the correct HWND and [overwrite review](xml-toolkit-overwrite.md). |
| XML-TOOL-05 | Preserve remembered Dock/Pop Out, independent native window and current editor, schema and reviewed payload through accepted rehost. Preserve startup/activity/failure and draining while rejecting stale operation results. |

Select `xml_toolkit_plugin_command_routes_once_through_the_typed_bus`, `m7f_xml_open_and_passive_frames_do_not_touch_clipboard_and_paste_reads_once`, `m7f_xml_session_workflow_uses_one_current_session_for_real_parse_xpath_format_xsd_and_reopen`, `m7f_xml_stale_load_completion_cannot_replace_edited_source_or_closed_draft`, `m7f_xml_output_apply_respects_source_limit_while_copy_and_save_keep_full_payload`, `m7d_namespace_row_actions_commit_once_when_egui_discards_a_pass` and `m7e_schema_view_discards_stale_results_and_clips_long_diagnostics`. `G14-EGUI-STRUCTURED-TEXT` verifies the native typed Source, parse, XPath and separate formatted output. Tree/validation, native I/O, host lifecycle and GPUI remain pending.
