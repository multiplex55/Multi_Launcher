# Parity matrix schema and initial state

Status: no accepted migration criteria. G00 expands the source discovery candidates into actual GUI records and rows; G04 adds the first implemented real GUI.

Use one criterion per meaningful behavior/control group or supported window mode, with enough granularity to expose omissions. Do not require a separate test for each row when a scenario proves several rows.

| Field | Meaning |
| --- | --- |
| Criterion ID | Stable ID such as GUI-012-KBD-03, assigned during inventory |
| GUI / parent / child / mode | Exact surface and its relationship |
| Requirement / baseline | User requirement and source/runtime expected behavior |
| Owner goal / checkpoint | Single accountable owner |
| Controls / interaction / outcome | What the user does and what must happen |
| Shared owner / frontend adapter | Domain operation and GPUI/egui obligations |
| Stage | Not started, Specified, In implementation, Implemented, Validation pending, Accepted |
| Evidence IDs | Tests, native/visual observations and binary/commit |
| Remaining gap / blocker | Precise missing behavior or evidence |
| Evidence applicability | Current, needs targeted revalidation, or superseded |
| Reviewer disposition | Scope checked and findings resolved |

## Cross-cutting criteria to instantiate

- PAR-01–PAR-10 from the master Goal.
- Startup truth table and existing-instance behavior from FRONTEND_SELECTION.
- Core command and outcome coverage, including default no-op host hazards.
- Data/store/config round trips and unsaved-edit semantics.
- Every GUI’s supported modes and child flows.
- Relevant real Windows input/window/clipboard/overlay/DPI behavior.
- Visual density/text-size/resizing/focus and applicable accessibility.
- No inactive frontend runtime or duplicate core/service implementation.
- Final artifact and performance evidence.

A GUI cannot be accepted while a mandatory row is merely Implemented or Validation pending. A user-approved exception is a separately reported scope change, not a passing row.

## Source-backed GUI criteria

| GUI | Criteria | Owner | Stage | Remaining evidence |
| --- | --- | --- | --- | --- |
| [Hash Generator](gui/hash-generator.md) | HASH-01–HASH-08 | G04 | In implementation | GPUI Text/File, algorithms, comparison, keyboard routes, owned pickers, retained rehost, streamed cancel and default/larger profiles observed. Native Clipboard Load/Copy and cross-frontend native popout geometry passed. Docked geometry, mixed DPI, hidden-root lifecycle, egui native clipboard and full visual/accessibility acceptance remain pending. |
| [Launcher](gui/launcher.md) | LAUNCH-01–LAUNCH-09 | G05 | Specified | Scoped egui keyboard baseline observed. GPUI product view, typed routing, global invocation, native keyboard and visual evidence remain pending. |
| [Coordinate overlay](gui/coordinate-overlay.md) | COORD-01–COORD-05 | G22 | Specified | Existing controller start, stop and DPI sampling observed from GPUI. Production command routes, visible HUD, pointer pass-through, capture behavior and mixed DPI remain pending. |
| [Main Settings](gui/settings-main.md) | SETTINGS-01–SETTINGS-06 | G06 | Specified | The egui toolbar, search result and footer were exposed on Windows. Destination focus, save/cancel round trip, nested ownership and the GPUI view remain pending. |
| [Theme Settings](gui/theme-settings.md) | THEME-01–THEME-05 | G06 | Specified | Native egui direct open, committed appearance, Apply, Save and Cancel observed. Main Settings child delivery, palette cases, GPUI editor and visual acceptance remain pending. |
| [File Search](gui/file-search.md) | FILE-SEARCH-01–FILE-SEARCH-06 | G07 | Specified | Native egui scoped filename search, result refinement and exact File Operations toolbar handoff observed. Pointer Hash handoff, content results, other state transitions and GPUI remain pending. |
| [Diff workbench](gui/diff.md) | DIFF-01–DIFF-08 | G08 | Specified | Native egui two-file Text comparison, first changed hunk and in-pane Find observed. Binary, Folder, child forms, watcher and operation behavior, native hosting and the GPUI view remain pending. |
| [Diff Comparison Rules](gui/diff-comparison-rules.md) | DIFF-RULES-01–DIFF-RULES-03 | G08 | Specified | GPUI form and native Apply, error and host behavior pending. |
| [Diff Folder Rules](gui/diff-folder-rules.md) | DIFF-FRULES-01–DIFF-FRULES-03 | G08 | Specified | GPUI form and native rescan, validation and host behavior pending. |
| [Diff Text Inputs](gui/diff-text-input.md) | DIFF-INPUT-01–DIFF-INPUT-03 | G08 | Specified | GPUI form and native Compare, Apply, Cancel and retained rehost pending. |
| [Diff Folder Operation Preview](gui/diff-folder-operation-preview.md) | DIFF-OP-01–DIFF-OP-03 | G08 | Specified | GPUI modal, reviewed plan retention and disposable mutation evidence pending. |
| [Diff Unsaved Comparison](gui/diff-unsaved-comparison.md) | DIFF-DIRTY-01–DIFF-DIRTY-03 | G08 | Specified | GPUI modal and native dirty-owner Save, Discard and Cancel pending. |
| [Batch Renamer](gui/batch-rename.md) | RENAME-01–RENAME-06 | G09 | Specified | Native egui launch, folder-picker dismissal, Prefix draft and scan gate observed. Successful scan/preview, mutations, other host modes and GPUI remain pending. |
| [Confirm Batch Rename](gui/batch-rename-confirm.md) | RENAME-APPLY-01–RENAME-APPLY-03 | G09 | Specified | Native frozen-plan decision and GPUI implementation pending. |
| [Confirm Batch Rename Undo](gui/batch-rename-undo-confirm.md) | RENAME-UNDO-01–RENAME-UNDO-03 | G09 | Specified | Native receipt-bound undo and GPUI implementation pending. |
| [Search and Replace](gui/search-replace.md) | REPLACE-01–REPLACE-06 | G09 | Specified | Native egui launch, text drafts, picker dismissal and explicit-root gates observed. Search/preview, other host modes and GPUI remain pending. |
| [Search and Replace Confirmations](gui/search-replace-confirm.md) | REPLACE-DECISION-01–REPLACE-DECISION-04 | G09 | Specified | Native Apply, Restore and Recovery decisions and GPUI remain pending. |
| [Folder Size](gui/folder-size.md) | FOLDER-SIZE-01–FOLDER-SIZE-05 | G10 | Specified | Source routes, worker and view modes mapped. Native prefill, scan and host proofs and GPUI remain pending. |
| [Duplicate Finder](gui/duplicate-finder.md) | DUPLICATE-01–DUPLICATE-05 | G10 | Specified | Native egui scan and exact group observed. Content after popout, remaining action and lifecycle paths and GPUI remain pending. |
| [ZIP Extractor](gui/zip-extractor.md) | ZIP-01–ZIP-06 | G10 | Specified | Native Triage handoff, extraction preview and empty Create gate observed. Guarded writes, other host/picker paths and GPUI remain pending. |
| [File Lock Inspector](gui/file-lock.md) | FILE-LOCK-01–FILE-LOCK-05 | G10 | Specified | Reported-use inspection, process identity and host actions mapped. Native GUI inspection and GPUI remain pending. |
| [Downloads Triage](gui/downloads-triage.md) | DOWNLOADS-01–DOWNLOADS-06 | G11 | Specified | Native egui launch, exact selection and ZIP handoff observed. Remaining actions, scan/view/host lifecycle and GPUI remain pending. |
| [File Operations Center](gui/file-operations-center.md) | FILE-OPS-01–FILE-OPS-05 | G11 | Specified | Native egui File Search toolbar delivery and read-only Duplicate review observed. Queue, execution, recovery, other routes and GPUI remain pending. |
| [Explorer Tabs](gui/explorer-tabs.md) | EXPLORER-TABS-01–EXPLORER-TABS-05 | G11 | Specified | Native editor, exact unsaved draft and Escape from the dirty decision observed. Saved-list run, owned Explorer verification and GPUI remain pending. |
| [Explorer Tabs Saved Folders](gui/explorer-tabs-saved-folders.md) | TAB-FOLDERS-01–TAB-FOLDERS-03 | G11 | Specified | Native refresh, alias filter and exact focused activation observed. Host lifecycle, stale cases and GPUI remain pending. |
| [Quick Notes](gui/quick-notes.md) | QUICK-NOTES-01–QUICK-NOTES-04 | G12 | Specified | Native launcher, source filtering and multiline Add Note/Cancel observed. Remaining row actions, save/host and GPUI remain pending. |
| [Note Editor](gui/note-editor.md) | NOTE-EDITOR-01–NOTE-EDITOR-04 | G12 | Specified | Native exact note route and Render/Edit/Split observed. Find showed `0/0` for visible text. Find navigation, other routes, save and GPUI remain pending. |
| [Markdown Studio](gui/markdown-studio.md) | MD-STUDIO-01–MD-STUDIO-05 | G12 | Specified | Native egui source, Preview and Split, publication child and dirty Discard observed. Remaining editor and host paths and GPUI remain pending. |
| [Export / Print](gui/markdown-print.md) | MD-PRINT-01–MD-PRINT-04 | G12 | Specified | Native egui Studio handoff, one-page preview and retained parent draft observed. Note route, output/chooser and host behavior and GPUI remain pending. |
| [Note Graph](gui/note-graph.md) | NOTE-GRAPH-01–NOTE-GRAPH-04 | G12 | Specified | Native tag/root prefill, Details navigation and exact linked Note Editor observed. Canvas gestures and GPUI remain pending. |
| [Unused Note Assets](gui/unused-note-assets.md) | NOTE-ASSETS-01–NOTE-ASSETS-03 | G12 | Specified | Native orphan inventory and close observed with unchanged assets. Delete, host and GPUI remain pending. |
| [Note Templates](gui/note-templates.md) | NOTE-TEMPLATES-01–NOTE-TEMPLATES-03 | G12 | Specified | Source route, draft, store/cache and host behavior mapped. Native and GPUI remain pending. |
| [Delete Template?](gui/note-template-delete.md) | NOTE-TEMPLATE-DELETE-01–NOTE-TEMPLATE-DELETE-02 | G12 | Specified | Source decision, exact target and cancellation mapped. Native and GPUI remain pending. |
| [Note Custom Table Builder](gui/note-custom-table-builder.md) | NOTE-TABLE-01–NOTE-TABLE-03 | G12 | Specified | Source editor/caret identity, bounded form and insertion mapped. Native and GPUI remain pending. |
| [Insert Link](gui/note-insert-link.md) | NOTE-LINK-01–NOTE-LINK-02 | G12 | Specified | Source editor, selection and Markdown insertion mapped. Native and GPUI remain pending. |
| [Link New Note](gui/note-link-new.md) | NOTE-LINK-NEW-01–NOTE-LINK-NEW-03 | G12 | Specified | Source child route, displayed note ownership and current target publication mapped. Native and GPUI remain pending. |
| [Note Close Decision](gui/note-close-decision.md) | NOTE-CLOSE-01–NOTE-CLOSE-03 | G12 | Specified | Native Keep Editing and exact unsaved Discard observed. Save, host variants and GPUI remain pending. |
| [Note Save Conflict](gui/note-save-conflict.md) | NOTE-CONFLICT-01–NOTE-CONFLICT-03 | G12 | Specified | Source occupied-path decision and accepted identity changes mapped. Native and GPUI remain pending. |
| [Convert Plain Links](gui/note-plain-link-conversion.md) | NOTE-CONVERSION-01–NOTE-CONVERSION-03 | G12 | Specified | Native one-URL review, Cancel and unsaved Convert observed. Stale review, other classes and GPUI remain pending. |
| [Clipboard History](gui/clipboard-history.md) | CLIP-HISTORY-01–CLIP-HISTORY-04 | G13 | Specified | Native synthetic store, Edit/Cancel and Remove/Cancel observed. OS effects, persisted mutations, host lifecycle and GPUI remain pending. |
| [Clipboard Modify](gui/clipboard-modify.md) | CLIP-MODIFY-01–CLIP-MODIFY-05 | G13 | Specified | Source editor, inline preview and protected authority mapped. Native and GPUI remain pending. |
| [Snippets](gui/snippets.md) | SNIPPETS-01–SNIPPETS-04 | G13 | Specified | Native list, exact editor, preview round-trip and unchanged store observed. Save, removal, host variants and GPUI remain pending. |
| [Snippet Prompt](gui/snippet-prompt.md) | SNIPPET-PROMPT-01–SNIPPET-PROMPT-04 | G13 | Specified | Native standalone Fill and editor Preview verified with field-only text. Clipboard effects, sequence companion and GPUI remain pending. |
| [Create Temp File](gui/create-tempfile.md) | TEMPFILE-01–TEMPFILE-03 | G13 | Specified | Native Unicode draft, Cancel and blank reopen observed. Actual creation, dashboard and GPUI remain pending. |
| [JSON Utility](gui/json-utility.md) | JSON-UTIL-01–JSON-UTIL-04 | G14 | Specified | Native pinned Text, Format, Tree, Unicode pointer and Minify observed. Clipboard path, host and GPUI remain pending. |
| [XML Toolkit](gui/xml-toolkit.md) | XML-TOOL-01–XML-TOOL-05 | G14 | Specified | Native source parse, XPath and separate formatting observed. Validation, native I/O, host and GPUI remain pending. |
| [XML Save Overwrite](gui/xml-toolkit-overwrite.md) | XML-OVERWRITE-01–XML-OVERWRITE-02 | G14 | Specified | Source owner, frozen write and confirmation mapped. Native and GPUI remain pending. |
| [Regex Tester](gui/regex-tester.md) | REGEX-01–REGEX-05 | G14 | Specified | Native Unicode evaluation/navigation, synthetic history and syntax error observed. Remaining sections, host and GPUI remain pending. |
| [Calculation Pad](gui/calculation-pad.md) | CALC-PAD-01–CALC-PAD-05 | G15 | Specified | Native two-line calculation, exact dependency handoff and dirty Discard observed. Complete editor/host and GPUI remain pending. |
| [Calculation Pad Examples & Reference](gui/calculation-pad-reference.md) | CALC-REF-01–CALC-REF-03 | G15 | Specified | Actual native Tools route opened the reference window. Child controls were truncated. Browse/insert, host and GPUI remain pending. |
| [Dependency Explorer](gui/calculation-pad-dependencies.md) | CALC-DEP-01–CALC-DEP-03 | G15 | Specified | Native direct upstream navigation selected exact Pad source. Inactive message on Pad close observed. Other native states and GPUI remain pending. |
| [Programmer Inspector](gui/programmer-inspector.md) | PROGRAMMER-01–PROGRAMMER-04 | G15 | Specified | Source width/format, comparison, bit labels, persistence and exact Pad insertion mapped. Native and GPUI remain pending. |
| [Replace Bit Labels?](gui/programmer-inspector-label-replace.md) | BIT-LABEL-REPLACE-01–BIT-LABEL-REPLACE-02 | G15 | Specified | Source exact native import confirmation and retained label identity mapped. Native and GPUI remain pending. |
| [Calculation Pad Charts](gui/calculation-pad-charts.md) | CALC-CHART-01–CALC-CHART-04 | G15 | Specified | Source scene/kinds, staged options, image authority and persistence mapped. Native and GPUI remain pending. |
| [Sensitivity Tables](gui/calculation-pad-sensitivity.md) | CALC-SENS-01–CALC-SENS-03 | G15 | Specified | Source definition, exact bounded evaluation, current table and Chart handoff mapped. Native and GPUI remain pending. |
| [Table Utility and CSV Viewer](gui/table-utility.md) | TABLE-UTIL-01–TABLE-UTIL-04 | G16 | Specified | Native CSV explicit parse, literal filter and selection count observed. Other profile, output/host and GPUI remain pending. |
| [Date & Time Studio](gui/date-time-studio.md) | DATE-STUDIO-01–DATE-STUDIO-04 | G16 | Specified | Native Between calculation and retained dates on reopen observed. Other modes/effects, host and GPUI remain pending. |
| [Convert Panel](gui/convert.md) | CONVERT-01–CONVERT-03 | G16 | Specified | Source physical/base modes, targets and explicit preferences mapped. Native and GPUI remain pending. |
| [Password Generator](gui/password-generator.md) | PASSWORD-01–PASSWORD-03 | G16 | Specified | Source policy, concealed identity and protected copy ownership mapped. Native and GPUI remain pending. |
| [Diagram Studio](gui/diagram-studio.md) | DIAGRAM-01–DIAGRAM-05 | G17 | Specified | Native Mermaid draft, ready state, viewer route and dirty Cancel/Discard observed. Canvas, I/O, return and GPUI remain pending. |
| [Diagram Viewer](gui/diagram-viewer.md) | DIAGRAM-VIEW-01–DIAGRAM-VIEW-03 | G17 | Specified | Native independent zoom, canceled parent decision and scoped close observed. Canvas, detached host and GPUI remain pending. |
| [Crop Image](gui/image-crop-editor.md) | CROP-IMAGE-01–CROP-IMAGE-04 | G18 | Specified | Native generated PNG picker, dimensions and exact overwrite Cancel observed. Handle/pointer, output, screenshot and GPUI remain pending. |
| [Image Comparison](gui/image-comparison.md) | IMAGE-DIFF-01–IMAGE-DIFF-03 | G18 | Specified | Native open, owned A picker Cancel and popout chrome observed. Pair load/evaluation, data-bearing host and GPUI remain pending. |
| [Image Viewer](gui/image-viewer.md) | IMAGE-VIEW-01–IMAGE-VIEW-03 | G18 | Specified | Native Note attachment opened the exact scratch PNG. Active viewer controls, detached lifecycle and GPUI remain pending. |
| [QR Generator](gui/qr-generator.md) | QR-01–QR-03 | G18 | Specified | Native payload metadata, correction, scratch PNG and fresh reopen observed. Clipboard, host and GPUI remain pending. |
| [Screen Color Picker](gui/screen-color-picker.md) | SCREEN-COLOR-01–SCREEN-COLOR-03 | G18 | Specified | Source checked native overlay, parking and RGB publication mapped. Native and GPUI remain pending. |
| [Screenshot Countdown](gui/screenshot-countdown.md) | SHOT-COUNT-01–SHOT-COUNT-03 | G18 | Specified | Source delayed capture, checked native cancel and coordinator mapped. Native and GPUI remain pending. |
| [Screenshot Region Selection](gui/screenshot-region-selection.md) | SHOT-REGION-01–SHOT-REGION-03 | G18 | Specified | Source external Windows selection, bounded clipboard wait and result delivery mapped. Native and GPUI remain pending. |
| [Screenshot Editor](gui/screenshot-editor.md) | SHOT-EDIT-01–SHOT-EDIT-03 | G18 | Specified | Source markup, source-pixel crop/history and explicit output mapped. Native and GPUI remain pending. |
| [Mouse/Keyboard Macros](gui/mkmacro.md) | MKMACRO-01–MKMACRO-04 | G19 | Specified | Native manager, unsaved macro draft, Delay route and Discard observed. Save, playback/recording, host and GPUI remain pending. |
| [MkMacro Action Catalog](gui/mkmacro-action-catalog.md) | MK-ACTION-CATALOG-01–MK-ACTION-CATALOG-02 | G19 | Specified | Native Add Action, Delay child route and Cancel retention observed. Other workflows, host and GPUI remain pending. |
| [MkMacro Action Editor](gui/mkmacro-action-editor.md) | MK-EDIT-01–MK-EDIT-03 | G19 | Specified | Native Delay editor, Random range controls and Cancel observed. Apply, other variants, host and GPUI remain pending. |
| [Macro Parameters](gui/mkmacro-parameter-prompt.md) | MK-PARAM-01–MK-PARAM-03 | G19 | Specified | Source required values, owned browse and exact invocation lease mapped. Native and GPUI remain pending. |
| [MkMacro Window Picker](gui/mkmacro-window-picker.md) | MK-WINDOW-PICKER-01–MK-WINDOW-PICKER-03 | G19 | Specified | Source scope, candidate/criteria draft and one retained target effect mapped. Native and GPUI remain pending. |
| [Import Macro Library](gui/mkmacro-package-import.md) | MK-IMPORT-01–MK-IMPORT-03 | G19 | Specified | Source preview, exact snapshot/publish and asset rollback mapped. Native and GPUI remain pending. |
| [Export Macro Library](gui/mkmacro-package-export.md) | MK-EXPORT-01–MK-EXPORT-02 | G19 | Specified | Source root/closure, exact chooser and output ownership mapped. Native and GPUI remain pending. |
| [Save Macro Template](gui/mkmacro-template-save.md) | MK-TEMPLATE-SAVE-01–MK-TEMPLATE-SAVE-02 | G19 | Specified | Source retained fields and independent catalog save mapped. Native and GPUI remain pending. |
| [Macro Templates](gui/mkmacro-templates.md) | MK-TEMPLATES-01–MK-TEMPLATES-02 | G19 | Specified | Source template review and independent copy mapped. Native and GPUI remain pending. |
| [Choose Launcher Action](gui/mkmacro-launcher-picker.md) | MK-LAUNCHER-PICKER-01–MK-LAUNCHER-PICKER-03 | G19 | Specified | Source exact authoring action, conversion decision and stale-owner guard mapped. Native and GPUI remain pending. |
| [MkMacro Step Search](gui/mkmacro-step-search.md) | MK-STEP-SEARCH-01–MK-STEP-SEARCH-03 | G19 | Specified | Source selection, Find/Jump and exact replacement preview mapped. Native and GPUI remain pending. |
| [MkMacro Help](gui/mkmacro-help.md) | MK-HELP-01–MK-HELP-02 | G19 | Specified | Source empty-library route, content and mounted help owner mapped. Native and GPUI remain pending. |
| [MkMacro Recording Annotation](gui/mkmacro-recording-annotation.md) | MK-RECORD-NOTE-01–MK-RECORD-NOTE-02 | G19 | Specified | Source exact timed note, pause and native owner mapped. Native and GPUI remain pending. |
| [MkMacro Recording Review](gui/mkmacro-recording-review.md) | MK-RECORD-REVIEW-01–MK-RECORD-REVIEW-03 | G19 | Specified | Source review queue, draft/target, local edits and accepted insertion mapped. Native and GPUI remain pending. |
| [MkMacro Runtime Prompt](gui/mkmacro-runtime-prompt.md) | MK-RUNTIME-PROMPT-01–MK-RUNTIME-PROMPT-02 | G19 | Specified | Source broker request, exact response and hidden-root owner mapped. Native and GPUI remain pending. |
| [MkMacro Reference Crop](gui/mkmacro-reference-crop.md) | MK-REF-CROP-01–MK-REF-CROP-03 | G19 | Specified | Source managed crop, exact target, save and replace decision mapped. Native and GPUI remain pending. |
| [Save Captured Reference Image](gui/mkmacro-reference-save.md) | MK-REF-SAVE-01–MK-REF-SAVE-02 | G19 | Specified | Source pending capture pixels, naming and one typed asset publication mapped. Native and GPUI remain pending. |
| [Reference Image Already Exists](gui/mkmacro-reference-overwrite.md) | MK-REF-REPLACE-01–MK-REF-REPLACE-02 | G19 | Specified | Source import collision, explicit decision and stale completion mapped. Native and GPUI remain pending. |
| [Legacy Macros](gui/legacy-macros.md) | LEGACY-MACRO-01–LEGACY-MACRO-03 | G19 | Specified | Native exact stored inert row, Edit/Cancel and unchanged JSON observed. Save/remove, execution and GPUI remain pending. |
| [Screen Recorder Setup](gui/screen-recorder-setup.md) | RECORD-SETUP-01–RECORD-SETUP-03 | G19 | Specified | Native duration gate, Unlimited and owned output cancellation observed. Active recording, host variants and GPUI remain pending. |
| [Recording Controller](gui/screen-recorder-controller.md) | RECORD-CONTROLLER-01–RECORD-CONTROLLER-03 | G19 | Specified | Source active generation, detached controls and staged retirement mapped. Native and GPUI remain pending. |
| [Radial Designer](gui/radial-designer.md) | RADIAL-DESIGNER-01–RADIAL-DESIGNER-03 | G20 | Specified | Native Menus and Skins route entries opened designer chrome with runtime disabled. Content, authoring, runtime and GPUI remain pending. |
| [Add to Radial](gui/radial-add-to-menu.md) | RADIAL-ADD-01–RADIAL-ADD-03 | G20 | Specified | Source stable primary/secondary/query action handoff and draft insertion mapped. Native and GPUI remain pending. |
| [Radial Cell Properties](gui/radial-cell-properties.md) | RADIAL-CELL-01–RADIAL-CELL-02 | G20 | Specified | Source scoped launcher binding, exact search and properties apply mapped. Native and GPUI remain pending. |
| [Radial Submenu Placement](gui/radial-submenu-placement.md) | RADIAL-PLACEMENT-01–RADIAL-PLACEMENT-02 | G20 | Specified | Source exact runtime failure recovery and correlated persisted change mapped. Native and GPUI remain pending. |
| [Radial Skin Editor](gui/radial-skin-editor.md) | RADIAL-SKIN-01–RADIAL-SKIN-03 | G20 | Specified | Native Skins route chrome observed. Controls, publish/host and GPUI remain pending. |
| [Radial Asset Picker](gui/radial-asset-picker.md) | RADIAL-ASSET-01–RADIAL-ASSET-03 | G20 | Specified | Source managed and separate external resource boundaries mapped. Native and GPUI remain pending. |
| [Unsaved Radial Changes](gui/radial-unsaved-changes.md) | RADIAL-CLOSE-01–RADIAL-CLOSE-02 | G20 | Specified | Source retained close decision, child staging and checked revert mapped. Native and GPUI remain pending. |
| [Unsaved Radial Cell Properties](gui/radial-unsaved-cell-properties.md) | RADIAL-CELL-DIRTY-01–RADIAL-CELL-DIRTY-02 | G20 | Specified | Source staged Properties and stale navigation decisions mapped. Native and GPUI remain pending. |
| [Radial Inspector Scope Change Guard](gui/radial-scope-change-guard.md) | RADIAL-SCOPE-01–RADIAL-SCOPE-02 | G20 | Specified | Source Inspector intent ownership and replay decisions mapped. Native and GPUI remain pending. |
| [Radial Delete Impact](gui/radial-delete-impact.md) | RADIAL-DELETE-01 | G20 | Specified | Source reference impact and delete refusal mapped. Native and GPUI remain pending. |
| [Delete Populated Radial Ring](gui/radial-delete-ring.md) | RADIAL-RING-DELETE-01 | G20 | Specified | Source explicit relocation, discard and atomicity mapped. Native and GPUI remain pending. |
| [Resolve Populated Radial Cells](gui/radial-resolve-populated-cells.md) | RADIAL-RING-RESOLVE-01 | G20 | Specified | Source checked proposal preview and explicit Apply mapped. Native and GPUI remain pending. |
| [Radial Runtime](gui/radial-runtime.md) | RADIAL-RUNTIME-01–RADIAL-RUNTIME-03 | G20 | Specified | Independent native host, input lifecycle and exact action handoff mapped. Native and GPUI remain pending. |
| [Radial Authoring Preview](gui/radial-preview.md) | RADIAL-PREVIEW-01–RADIAL-PREVIEW-03 | G20 | Specified | Embedded and checked desktop preview modes mapped. Native and GPUI remain pending. |
| [Radial Import and Export](gui/radial-import-export.md) | RADIAL-PACKAGE-01–RADIAL-PACKAGE-03 | G20 | Specified | Package and legacy review, backup, export and two source-observed lifetime risks mapped. Native and GPUI remain pending. |
| [Paste Sequences](gui/paste-sequences.md) | PASTE-SEQUENCE-01–PASTE-SEQUENCE-04 | G21 | Specified | Source editor, preflight, playback and retained host mapped. Native and GPUI remain pending. |
| [Mouse Gestures](gui/mouse-gestures.md) | GESTURE-EDITOR-01–GESTURE-EDITOR-03 | G21 | Specified | Native editor, default add and rename cancellation verified with runtime disabled. Capture and GPUI remain pending. |
| [Bind Mouse Gesture Action](gui/mouse-gesture-bind-action.md) | GESTURE-BIND-01–GESTURE-BIND-02 | G21 | Specified | Actual retained child and cancellation verified. Save and GPUI remain pending. |
| [Mouse Gesture Settings](gui/mouse-gesture-settings.md) | GESTURE-SETTINGS-01–GESTURE-SETTINGS-02 | G21 | Specified | Source dedicated settings and live versus saved behavior mapped. Native and GPUI remain pending. |
| [Mouse Gesture Settings Window Picker](gui/mouse-gesture-window-picker.md) | GESTURE-WINDOW-01–GESTURE-WINDOW-02 | G21 | Specified | Source accepted title and parent identity checks mapped. Native and GPUI remain pending. |
| [Mouse Gesture Plugin Ignore Picker](gui/gesture-ignore-window-picker.md) | GESTURE-PLUGIN-IGNORE-01 | G21 | Specified | Source and synthetic fixture identified. Production entry and GPUI remain pending. |
| [Mouse Gesture Hint Overlay](gui/mouse-gesture-hint-overlay.md) | GESTURE-HINT-01–GESTURE-HINT-02 | G21 | Specified | Existing Windows passive host and precise runtime lifecycle mapped. Native and GPUI remain pending. |
| [Hot Folder Manager](gui/hot-folder-manager.md) | HOT-FOLDER-01–HOT-FOLDER-03 | G21 | Specified | Native empty manager and disabled draft cancellation passed. Save, watches and GPUI remain pending. |
| [Hot Folder Target Picker](gui/hot-folder-target-picker.md) | HOT-FOLDER-TARGET-01–HOT-FOLDER-TARGET-02 | G21 | Specified | Native registered launcher action picker and Escape passed. Binding/other choices and GPUI remain pending. |
