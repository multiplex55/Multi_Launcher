# G12 — Notes, Markdown Studio, graph, export, and printing

**Status:** Pending. **Owner:** G12 implementer with note/document/persistence owners. No migration edits or tests have been executed. Use the established migration branch.

## Objective and dependencies

Recreate the complete note and Markdown working environment in GPUI Kit, preserving authoring behavior, saved documents/assets, navigation, previews, synchronization, and publication outputs. A generic Kit Markdown viewer does not establish parity with Multi_Launcher's note semantics and editing workflows.

Depend on G00–G04 and G05/G06 routes and settings registration. Coordinate note–todo/calendar/widget handoffs with G23 and diagram-source editing with G17. G12 owns its specialized note/Markdown settings and children; G06 owns their shell. Follow [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Pinned baseline: `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| Responsibility | Existing source |
|---|---|
| Notes and editing | [src/gui/note_panel.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/note_panel.rs), `notes_dialog.rs`, `note_form_dialog.rs`, `note_find.rs`, `note_find_preview.rs`, `note_mutation.rs`, `note_clipboard.rs` |
| Graph/assets | `src/gui/note_graph_dialog.rs`, `unused_assets_dialog.rs`, `src/graph/note_graph.rs`, `src/note_ui_state.rs` |
| Markdown Studio | [src/gui/markdown_studio.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/markdown_studio.rs), `markdown_outline.rs`, `markdown_preview.rs`, `src/markdown_studio/` |
| Publication/shared semantics | `src/gui/markdown_export.rs`, `markdown_print.rs`, `markdown_print/{dialog,output,printer_dialog}.rs`, `src/notes_markdown/`, `src/note_todo_sync/`, `src/plugins/note.rs` |

Inspected sources show separate Quick Notes and note-editor native identities, document/view selection, source-snapshot-bound diagram fences, image caches, find/preview interactions, and significant egui-specific rendering. The requested architecture reuses document/parse/mutation behavior while recreating those presentation mechanisms.

## Per-GUI scope

Include Quick Notes, note add/edit forms, all Note Panel modes, note find and preview highlighting/navigation, tags/links/backlinks as exposed, graph view, unused-assets review, and all associated menus/confirmations. Preserve autosave, explicit save/cancel/close behavior, drafts, selection/caret, scroll, and note identity.

Include Markdown Studio source/preview/split arrangements supported by the reference, formatting actions, outline navigation/reordering, tables/tasks/callouts/assets, diagram/Mermaid presentation and editing handoff, file open/save/save-as decisions, export, print preparation, print/printer dialogs, and output/error states. Inventory actual supported syntax and publication options; do not equate a third-party Markdown default with the existing product contract.

## Bounded checkpoint packets

### A — Establish document, mutation, and publication boundaries

**Owner:** G12 with the document engine owner. Trace note identities, document revisions, accepted mutations, autosave timing, asset references, preview caches, export snapshots, and note–todo synchronization. Extract behavior out of GUI handlers as needed; retain authoritative parsers, writers, and link/asset rules.

**Invariants:** Display refresh never overwrites an unsaved document or resets its caret. A stale action cannot edit another note or diagram fence. Source text, task state, and asset identity remain lossless. **Evidence and acceptance:** Contract tables and existing note persistence/mutation regressions. **Commit boundary:** Required neutral session operations and egui adapters, without a new document format.

### B — Complete Notes, graph, and asset workflows

**Owner:** G12 frontend implementer. Recreate Quick Notes and note editing with all fields, lists, navigation, find results, graph controls, asset review, and confirmations. Preserve supported editing/rendered modes and exact actions. Implement stable rows and caches for large note collections.

**Invariants:** List refresh and filtering preserve intended selection. Wrapping links or changing view mode does not open/close or overwrite the wrong panel. Asset review actions operate on the reviewed snapshot and keep current confirmation rules.

**Evidence and acceptance:** Disposable notes exercise creation/editing, autosave, cancel/close, links, tasks, find, graph navigation, long content, and asset review. **Commit boundary:** Complete Notes family with native and focused regression evidence; Markdown Studio remains separately tracked.

### C — Complete Markdown Studio and publication children

**Owner:** G12 frontend implementer with publication/native adapters. Port editor tools, source/preview layout, outline and formatting, file decisions, diagram-fence handoffs, exports, and print/printer interfaces. Compose custom GPUI elements where Kit components cannot retain the behavior. Reuse shared publication generation rather than screenshots of an egui view.

**Invariants:** Open/save picker completions belong to the initiating document and owner. Export/print uses the intended source snapshot and asset rules. Rendered preview selection and source selection retain their existing relationship. A diagram result cannot patch a changed source fence without existing validation.

**Evidence and acceptance:** Fixed Markdown fixtures cover supported syntax, Unicode, local assets, long documents, unsaved decisions, and export equivalence. Native print-dialog/output checks use controlled fixtures; unavailable printer capabilities are recorded, not assumed. **Commit boundary:** Studio plus publication children with separately identified outputs and remaining integration rows.

### D — Verify cross-window, cross-feature, and cross-frontend continuity

**Owner:** G12 acceptance owner with G17/G23. Exercise host changes, independent supported note windows, root-hidden save/completion, close/reopen, diagram edits, task synchronization, and widget/launcher opening routes. Restart between frontends and confirm the same notes/assets/view preferences remain usable.

**Invariants:** Background completion does not revive a retired editor; authoritative content is not duplicated across frontends. Source ownership stays exact across nested children. **Evidence and acceptance:** Native interaction records, persistence/output comparisons, and integration traces; dependencies awaiting G17/G23 remain pending until real execution. **Commit boundary:** Final family fixes and evidence, not unrelated note enhancements.

## Focused validation

Existing standalone sources include `tests/note_panel_auto_save.rs`, `note_panel_scroll.rs`, `note_panel_shell.rs`, and `notes_plugin.rs`; domain cases include `note_graph_settings.rs`. The validation survey also identified `gui::notes_dialog::tests::quick_notes_wrap_links_preserves_open_panel_and_view_mode`. Select/list the affected targets and exact inline cases; discover Markdown export/print, mutation, and asset tests before execution. Add GPUI-specific checks for cursor/selection retention and accepted revision-bound actions. Headless publication tests do not prove native printing, text input, focus, or popout behavior. Avoid a full repository suite for each editor subtask.

## Exit and continuation

Every owned editor, graph, asset, export, print, and settings surface has behavior/control/window evidence. Actual cross-feature routes remain explicitly open until their owners are available; no snapshot placeholder counts as a completed editor. Preserve egui read/write compatibility and continue G13 onward. Broader document-tool integration and final performance acceptance remain part of G25–G26.
