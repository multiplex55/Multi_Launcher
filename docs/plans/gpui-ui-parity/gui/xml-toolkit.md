# XML Toolkit

G14 owns `xml-toolkit`. Bare `xml` emits `xml:open`. `XmlToolkitDialogState` owns Source, Tree, XPath, Validation and Output and their exact native owner. The bounded XML engine/session own parsing, jobs, validation and derived output.

| Criterion | Source-backed behavior |
| --- | --- |
| XML-TOOL-01 | Open or focus the retained Source with no implicit clipboard read. Support typed text, explicit protected Paste, Copy Source, owned Open XML and Clear. Parse bounded local UTF-8, accept supported input BOM and disallow DTD/external resource resolution. Expired load cannot replace a changed source or retired owner. |
| XML-TOOL-02 | Tree presents bounded current nodes/details, expand/select and explicit source, fragment and value copy. XPath supports current expression and namespace mappings, scalar/node/empty/error states, navigation/reveal and exact selected result copy. |
| XML-TOOL-03 | Owned Choose XSD, Clear and Validate use the bounded local root-confined schema policy and exact current source/schema revisions. Suppress stale diagnostics and bound displayed errors. |
| XML-TOOL-04 | Format and Minify produce a separate bounded preview. Keep exact full output for Copy or Save. Apply Output is explicit and enforces the source limit. Save Source As and Save Output As use the correct HWND and [overwrite review](xml-toolkit-overwrite.md). |
| XML-TOOL-05 | Preserve remembered Dock/Pop Out, independent native window and current editor, schema and reviewed payload through accepted rehost. Preserve startup/activity/failure and draining while rejecting stale operation results. |

Select `xml_toolkit_plugin_command_routes_once_through_the_typed_bus`, `m7f_xml_open_and_passive_frames_do_not_touch_clipboard_and_paste_reads_once`, `m7f_xml_session_workflow_uses_one_current_session_for_real_parse_xpath_format_xsd_and_reopen`, `m7f_xml_stale_load_completion_cannot_replace_edited_source_or_closed_draft`, `m7f_xml_output_apply_respects_source_limit_while_copy_and_save_keep_full_payload`, `m7d_namespace_row_actions_commit_once_when_egui_discards_a_pass` and `m7e_schema_view_discards_stale_results_and_clips_long_diagnostics`. `G14-EGUI-STRUCTURED-TEXT` verifies stock native typed Source, parse, XPath and separate formatted output. Paired desktop evidence remains pending.

## GPUI Source and session

Stock typed Search opens XML Toolkit and focuses the retained draft. Source revisions publish only current diagnostics from the real Activity-backed engine. Paste explicitly reads protected Unicode text and uses the stock size and UTF-8 declaration policy. Copy Source preserves exact authored text, including invalid XML. Closing retires the current worker and effects, while reopening preserves the draft. Accepted launcher Quit and restart wait for worker retirement. The focused batch passed the real Windows engine gate. Matched desktop checks remain pending.

## GPUI Format and Output

Format and Minify use the exact current parsed source. The Output tab previews up to 8,000 Unicode characters. Copy writes the full current output explicitly, and Apply uses the 4 MiB source limit. The shared session rejects older completions even when successive requests use the same format mode. Engine and receipt tests passed. Compare mixed content, exact full Copy, and explicit Apply in the matched desktop run.

## GPUI Tree

The Tree tab uses the stock projection and one-shot inspection model. Virtual rows use typed document and node identities. The view preserves child order and omission markers, bounds the details preview, and copies the full value or fragment only on explicit request. The focused batch passed the real Windows Tree engine and the stock XPath consumers. Paired desktop replay still needs the namespaced, repeated-sibling and long-value fixture.

## GPUI XPath

The XPath tab uses the stock expression and namespace request model. It distinguishes empty matches, scalar output and errors. Virtual nodes support wrap navigation and exact Tree reveal. Copy uses full scalars or the exact current node through the shared clipboard lane. Focused namespace, request, navigation and Windows engine tests passed. Paired desktop replay still needs prefixed nodes and attributes, scalar and empty results, invalid expressions, and exact copies after navigation.

## GPUI Validation

The Validation tab chooses a local XSD through the checked native owner, retains the selected path, and validates only on demand through the existing engine. Both frontends share bounded status and diagnostic rules. Four focused launcher tests cover owner, source, mount and session receipt checks. The grouped replay still needs actual picker ownership, Cancel, Clear and validation against the relative and missing include fixtures.

## GPUI file effects

Open uses the checked native picker and shared bounded UTF-8 loader. Source and full Output can be saved through an owned picker. An existing destination opens the exact overwrite review. The grouped file and XSD tests passed, including Windows retry authorization and stock confirmation behavior. Verify actual picker ownership in both presentations in the paired desktop replay.

## GPUI retained host

Fresh typed Open uses the saved presentation, with Pop Out as the stock default. Dock and Pop Out retain the same engine, draft and tab state. The popup starts hidden, restores saved geometry before show and uses the shared native geometry observer. Dock requires exact launcher visibility and foreground acknowledgement before retiring its source. A rejected transfer restores the original mount. New edits and effects wait for transfer to finish, and picker and accepted write authority stay with the original mount. An open [Overwrite review](xml-toolkit-overwrite.md) moves with its frozen target and payload. Saved pins restore the editor passively, retain the workspace during pinned Close, and close the exact mount after the canonical unpin acknowledgement. Paired native geometry, foreground, close, picker, review and pin verification remain pending.

## GPUI scoped Jump

Source, XPath tab, XPath expression, Run XPath and Save Source As hints focus eligible controls in the active mount. Their existing handlers require fresh activation. An active chooser or exact Overwrite review suspends the parent scope. Focused input tests passed. The modal targets, additional controls and matched Windows replay remain pending. [Checkpoint](../CHECKPOINTS.md#g14-d-xml-workspace-jump)
