# G17 — Diagram Studio and every child surface

**Status: not started.** All checkpoints below are not started. Execute on the initiative's single branch. egui remains the no-argument default; GPUI is an explicitly selected native frontend.

## Objective and dependencies

Recreate the complete Diagram Studio experience in GPUI Kit: Mermaid authoring and preview, native Visual documents, viewers, document transitions, properties, templates, and import/export interactions. Preserve the existing capabilities and supported syntax. Visual polish may change drawing and typography, but must not remove an editing operation, alter document meaning, or replace the studio with a preview-only screen.

Requires the applicable G00–G04 foundation gates and G05 common routes, utility hosts, actions, focus, dialogs, and resource ownership. Use [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md). Reuse the shared text-editing and asset adapters established elsewhere; record a dependency if a required adapter is incomplete. Do not introduce a WebView to obtain Mermaid rendering.

## Source seeds and architectural ownership

Baseline: `multiplex55/Multi_Launcher` at `f7bc610a86bcad1c14b69dce65b1d804df1b7934`. Reconcile later source changes before implementation.

- [Studio UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/diagram_studio.rs): menus, canvases, retained edits, hosts, child presentations, and existing behavior tests.
- [Document controller](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/diagram/document.rs) and [session](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/diagram/session.rs): document identities, dirty transitions, and history.
- [Visual canvas](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/diagram/visual_canvas.rs), [rendering](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/diagram/rendering.rs), and [Mermaid](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/diagram/mermaid.rs): preserve domain geometry, accepted syntax, and render semantics.

The existing diagram subsystem owns documents, conversions, persistence, export, and history. The GPUI adapter owns input dispatch, focus, textures, window lifecycle, and presentation. Extract UI-coupled state only when necessary; do not create a second diagram format or parallel editor engine.

## Per-GUI parity scope

Create separately tracked parity rows for Mermaid Source, Split, and viewer modes; Visual canvas; detached viewers; source assistance and Colors; properties sidebar; template interactions; save/discard/cancel prompts; import reports; and export destinations/warnings. Follow every child-opening call in the studio rather than treating the parent window as the full feature.

Preserve File/Edit/View/Templates/Help menus and the action strip; source restoration; resizable independently scrolling properties; tool and zoom controls; source selections and shortcuts; suggestion application; render-pending and error states. Preserve source text when parsing cannot represent it, and preserve the established distinction between conversion, copying, and editing the original.

For Visual documents include object creation, selection and multiselection, marquee, dragging, resizing, snapping, groups/layers where present, alignment/distribution, minimap, inline labels, connector creation and endpoint reassignment, connector properties, and clipboard/history operations. Reproduce interaction precedence: text focus must protect text from canvas shortcuts, Escape must cancel an owned gesture before closing, and a late pointer release must not mutate a replaced document.

## Checkpoints

### G17-A — Map documents, child hosts, and editing transactions

**Status: not started.** **Owner:** the G17 implementer, with document behavior owned by `src/diagram/`.

**Work:** inventory routes, modes, child windows, commands, formats, and transition prompts; capture representative egui evidence; identify which existing state can be reused without GUI types. Define adapters for document identity, pending work, and commit/cancel effects.

**Invariants:** no new file format; no silent source conversion; stale document generations cannot accept edits.

**Evidence:** source-to-parity rows, reference captures, and targeted existing controller tests.

**Done:** every reachable surface has an owner and the first GPUI host opens the existing document model without duplicating persistence.

### G17-B — Recreate Mermaid editing and document workflows

**Status: not started.** **Owner:** GPUI studio/source-view adapter.

**Work:** implement source, split, viewer, menus, suggestions, Colors, templates, file operations, progress, errors, and dirty-transition decisions. Connect current rendering jobs and bounded image resources. Preserve state across docking/pop-out and resizing.

**Invariants:** canceled operations retain the draft; old render results cannot overwrite newer source; merely rendering a frame causes no export or save.

**Evidence:** production-view tests for edits and transitions; source/export comparisons; native text/focus review.

**Done:** Mermaid workflows and every associated child surface are usable through GPUI without an egui fallback window.

### G17-C — Recreate Visual editing and viewers

**Status: not started.** **Owner:** GPUI canvas/properties adapters; existing visual operations remain authoritative.

**Work:** implement selection, transforms, connectors, text, styling, assets, layers/groups, history, clipboard, and viewers according to the inventory. Preserve separate scrolling and toolbar reachability at supported sizes.

**Invariants:** one committed gesture creates the existing logical transaction; canceled/discarded gestures commit nothing; preview and export reflect the same document.

**Evidence:** pointer and keyboard interaction tests, reference documents, native canvas review, and representative export checks.

**Done:** all Visual parity rows, including connector endpoint reassignment and inline text cancellation, meet their acceptance cases.

### G17-D — Close parity and preserve the fallback

**Status: not started.** **Owner:** G17 implementer; bounded independent review for document/gesture ownership.

**Work:** resolve residual child dialogs, errors, focus, performance, and saved-document compatibility; exercise egui after shared changes.

**Invariants:** egui still opens the same saved documents; GPUI never opens an egui child; preview work remains bounded.

**Evidence:** completed parity rows, scoped test results, native screenshots, and representative large-document observations.

**Done:** all mandatory rows are verified, defects resolved, and coherent checkpoint commits recorded.

## Focused validation and continuation

Existing tests to locate by exact name include `visual_owned_label_inline_edit_commit_reedit_cancel_and_stale`, `visual_clipboard_events_deduplicate_and_failed_cut_preserves_scene`, `diagram_studio_visual_canvas_pointer_drag_moves_selection_once_and_supports_undo_redo`, and `diagram_studio_visual_canvas_text_focus_blocks_pointer_transform`. Discover the current compiled targets before selecting nextest filters; migrate behavior coverage rather than copying egui mechanics. Add GPUI production-view cases only for newly changed boundaries.

This goal completes after source/visual modes, children, persistence, exports, and applicable native interaction evidence pass. A working canvas alone is insufficient. Update the execution ledger, commit accepted checkpoints, and continue to the next ready goal. Record genuine blockers with preserved work and an exact next action; do not declare omitted modes complete or run the full nextest suite after each widget.
