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
| [Hash Generator](gui/hash-generator.md) | HASH-01–HASH-08 | G04 | In implementation | GPUI Text/File, algorithms, comparison, keyboard routes, owned pickers, retained rehost, streamed cancel and default/larger profiles observed. Native Clipboard Load/Copy round-trip passed. Geometry, hidden-root lifecycle, the egui native clipboard baseline and full visual/accessibility acceptance remain pending. |
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
| [Note Close Decision](gui/note-close-decision.md) | NOTE-CLOSE-01–NOTE-CLOSE-03 | G12 | Specified | Source close, configured save and exact owner admissions mapped. Native and GPUI remain pending. |
| [Note Save Conflict](gui/note-save-conflict.md) | NOTE-CONFLICT-01–NOTE-CONFLICT-03 | G12 | Specified | Source occupied-path decision and accepted identity changes mapped. Native and GPUI remain pending. |
| [Convert Plain Links](gui/note-plain-link-conversion.md) | NOTE-CONVERSION-01–NOTE-CONVERSION-03 | G12 | Specified | Source review, stale-source reconfirmation and draft mutation mapped. Native and GPUI remain pending. |
