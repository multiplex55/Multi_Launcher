# G14 — JSON Utility, XML Toolkit, and Regex Tester

**Status:** In implementation. **Owner:** G14 implementer with existing transform/session owners. JSON and XML workspace controls use the shared engines. Host and pin work continues before the Regex Tester batch.

## Objective and dependencies

Recreate all three structured-text workspaces in GPUI Kit, including editors, inspectors, secondary modes, templates/presets, help, validation, and output operations. Retain the existing parsers, regex semantics, XML engine session, limits, error messages, and source-authority rules. Do not substitute Kit's default interpretation for application behavior.

Depend on G00–G04, G05 routing, and G13's shared clipboard adapter where relevant. Own specialized tool settings/children; G06 supplies registration and settings hosting. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

All paths refer to baseline `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| Family | Existing source |
|---|---|
| JSON | [src/gui/json_utility_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/json_utility_dialog.rs), `src/json_transform.rs`, `src/commands/handlers/json_utility.rs` |
| XML | [src/gui/xml_toolkit_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/xml_toolkit_dialog.rs), `xml_toolkit_dialog/files.rs`, `xml_toolkit_{tree,xpath,schema}.rs`, `src/xml_toolkit_engine.rs`, `src/xml_toolkit_session.rs` |
| Regex | `src/gui/regex_tester_dialog.rs`, its `editor_layout`, `keyboard`, `inspection`, `substitution`, `test_cases`, `presets`, `history`, `reference`, `examples`, `explanation`, `visualization`, `profiling`, `highlighting`, `clipboard`, and `presentation` modules; `src/regex_tester/` |

Full primary dialog reads confirm worker generations/revisions, retained source drafts, bounded/virtual display, and explicit copy behavior. XML uses a real session lifecycle with starting/active/closing/failed states; Regex Tester is substantially more than a pattern-and-input pair.

## Per-GUI scope

JSON includes Text/Tree views, Format/Minify, explicit source reload/clear/copy, typed tree paths, expansion/selection, details, search navigation, match reveal, and distinct string-value versus subtree copy.

XML includes Source, Tree, XPath, Validation, and Output tabs; file load/save/overwrite decisions; source/output operations; engine startup/close/failure; full-payload output versus bounded editor limits; associated activity/status. Preserve actual schema and XPath functionality.

Regex includes raw pattern/flags and subject editors, matches/captures, navigation/highlights, substitution, saved presets/history, test cases, reference/examples, explanation/visualization/profiling, collapsed information sections, explicit copy, and all native/docked modes. Inventory every mode's controls, menus, keyboard bindings, loading/invalid/oversized/truncated states, and specialized settings.

## Bounded checkpoint packets

The first functional batch delivers JSON Utility, XML Toolkit and its XML Save Overwrite child. Carry the complete source-backed criteria through five increments, sharing the utility host, editor and completion handling where both tools need them.

| Increment | Deliverable and criteria |
| --- | --- |
| 1 | Shared JSON session and docked Text workspace through stock typed Search. Cover multiline editing, Format, Minify, protected Paste and Copy, Clear, diagnostics, close and fresh open for `JSON-UTIL-01` and the affected parts of `JSON-UTIL-04`. |
| 2 | JSON Tree, literal search, current details and typed copies. Finish saved Dock, Pop Out, pin and exact focus for `JSON-UTIL-02`–`04`. |
| 3 | XML Source using the real engine session. Cover protected Paste, Activity, startup failure, close while starting, reaping and reopen for `XML-TOOL-01` and `05`. |
| 4 | XML Tree, XPath, Validation and Output using current engine receipts. Cover Choose XSD, explicit copies and bounded preview versus full payload for `XML-TOOL-02`–`04`. |
| 5 | XML file Open, Save and Overwrite using the existing file-effect authority. Freeze review payloads and preserve no-clobber creation and explicit atomic replacement for `XML-TOOL-01`, `04`, `05` and `XML-OVERWRITE-01`–`02`. |

Use focused checks as worker ownership and file effects change. Group the remaining Nextest selectors per target and use one paired 1516 × 937 Windows candidate where practical. Packet C follows as the next functional batch and retains `REGEX-01`–`05`. Packet D applies across both batches.

Continue the retained host follow-up in order. First, XML uses the same checked Dock visibility acknowledgement, native geometry binding and retirement helpers as JSON. Second, transfer the open XML Overwrite review with the frozen payload and a new presentation receipt. Third, restore saved JSON and XML pins, reject pinned workspace close and accept unpin only after the checked settings write. Verify actual move, picker ownership, review transfer, passive startup, checked unpin, and exact native retirement with the paired profiles in the G14 Windows batch.

### A — Complete JSON source/tree interactions

**Owner:** G14 frontend implementer with the JSON operation owner. Map source edits, parsing/projection, search revisions, transforms, copy, and host actions into neutral intent/state contracts; keep the existing domain implementation. Recreate text editing and tree/search/details using Kit and bounded custom rows where needed.

**Invariants:** Parse failure preserves exact input and error location. Formatting/minifying does not implicitly copy. Tree actions cannot use pending, invalid, or stale snapshots. Expansion/selection follows typed paths rather than display row indices; missing paths are retired. Idle frames do not rebuild projections unnecessarily.

**Evidence and acceptance:** Fixtures exercise invalid/valid transitions, long Unicode values, collapsed search matches, typed paths, explicit copy, and source changes during work. **Commit boundary:** JSON contract adaptation, complete GUI, and its focused evidence; split by text versus tree if needed, keeping unfinished rows visible.

### B — Complete XML session and all tabs

**Owner:** G14 with the XML session/native adapter owner. Port source/tree/XPath/schema/output controls, status, files and confirmations through the existing session and file workers. Separate GPUI wake/owner mechanics from document and engine commands. Preserve startup, pending close, failure, and reopen behavior.

**Invariants:** A delayed file load cannot replace newer text or a closed draft. Host movement preserves the active session and frozen overwrite target. Output that cannot be applied to a bounded source editor remains available for supported copy/save. Passive frames do not add clipboard reads.

**Evidence and acceptance:** Real engine fixtures cover parse, XPath, formatting, schema validation, failure feedback, stale load, overwrite cancellation, close-during-startup, and reopen. **Commit boundary:** Complete XML GUI and child decisions with engine/session evidence; unavailable required engine prerequisites remain blocked rather than mocked.

### C — Complete the full Regex Tester workspace

**Owner:** G14 frontend implementer with the regex session owner. Recreate all inventoried subpanels and editing tools, including saved state, test cases, substitution, inspection, visualization, and reference/help. Preserve raw pattern handling and current unsupported-construct errors; retain the established regex engine and resource policy.

Begin with the stock `regex` Search route, retained host, live Pattern and flags, Test Text, match and capture inspection, navigation and automatic accepted-history recording. Continue with substitution, Test Cases and explicit protected clipboard. Bring History and Presets through the canonical stores, then add explanation, syntax structure, reference, examples and the remaining collapsed tools. Group focused linking at the functional boundaries and keep the shared host and native replay criteria in packet D.

**Invariants:** Selecting a match/capture changes navigation without editing source. Empty/unmatched captures and Unicode ranges remain correct. Truncated output is labeled as displayed output. Oversized source remains retained with a bounded preview; idle frames do not reschedule evaluation. Opening, rendering, or inspecting does not implicitly access the clipboard.

**Evidence and acceptance:** Fixture patterns exercise captures, empty matches, Unicode, invalid/unsupported constructs, replacement, test-case editing, preset removal cancellation, history, and oversized inputs. **Commit boundary:** Independent complete subpanel commits followed by Regex workspace integration; a basic matcher alone cannot close packet C.

### D — Verify common input, host, and effects behavior

**Owner:** G14 acceptance owner. Test text selection/composition, keyboard navigation, Jump Mode, nested popups, Alt+Space, small-window reachability, popout/dock, hidden-root completion, and close/reopen with pending work across all three families. Compare source/output bytes or normalized domain results against baseline fixtures and egui.

**Invariants:** Custom editor/navigation behavior never steals ordinary editing input; late effects belong to a live current owner and publish once. **Evidence and acceptance:** Native Windows recordings, action/clipboard counters, persisted-state comparisons, and row-level results. **Commit boundary:** Remaining family-specific fixes and evidence; invalidate affected earlier host/input rows if adapters change.

## Focused validation

Verified inline names include `worker_discards_stale_results_and_coalesces_pending_source` (JSON), `tree_copy_actions_are_explicit_and_keep_string_value_vs_subtree_semantics`, `m7f_xml_session_workflow_uses_one_current_session_for_real_parse_xpath_format_xsd_and_reopen`, `m7f_xml_stale_load_completion_cannot_replace_edited_source_or_closed_draft`, and `regex_tester_open_render_and_inspection_never_access_clipboard_implicitly`. Standalone XML sources include `tests/xml_engine_gate.rs` and `tests/xml_formatting.rs`. List exact nonzero affected selections before running; discover additional subpanel cases locally. Existing egui tests protect baseline contracts but require GPUI equivalents for real input and native hosting. Use targeted batches, not repeated full nextest runs.

## Exit and continuation

Every tab, inspector, saved-state editor, help surface, specialized setting, file decision, and assigned window mode is implemented and evidenced. No simplified regex/XML/JSON demo substitutes for the full inventory. Continue G15 and later integration, preserving shared input/editor fixes for affected earlier goals and keeping unavailable native/engine checks explicitly pending.
