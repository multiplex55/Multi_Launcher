# Diagram Studio

Diagram Studio edits the native Mermaid flowchart subset through its source
editor and graph canvas. It also previews a bounded set of additional Mermaid
families from their source. The [subset reference](DIAGRAM_MERMAID_SUBSET.md)
describes the distinction, syntax limits, and source bounds.
The [export reference](DIAGRAM_EXPORT.md) describes current-only native scene
capture, rendered SVG/PNG output limits, and exact-draft Mermaid export.

Use **Source**, **Visual**, or **Split** to choose an editing view. A supported
source change is parsed into the same graph used by the canvas. If the source
is invalid or contains unsupported native flowchart syntax, the raw text stays
available and the last valid native graph is shown as stale; graph changes
remain disabled until the source is valid again.

Mindmap, Sequence, Class, State, Entity Relationship, Gantt, Timeline, User
Journey, Pie, and GitGraph use the bounded source renderer. A current rendered
image appears in the preview area with Fit, zoom, and pan controls. These source
families do not create native graph nodes, an inspector, or Auto-layout
operations. If a new source revision is waiting for a render or fails, the
previous image remains visible with a stale indicator. The source stays
editable, and image export becomes available only for the render matching the
current document and source revision.

The visual inspector lists nodes, edges, and groups. Select an item there or on
the canvas to edit its properties. Shift-click or Ctrl-click nodes to build a
multi-selection. Drag selected nodes together, or use the arrow keys to move
them by 10 scene units; Shift+Arrow moves them by 40. With the canvas focused,
Ctrl+Arrow pans by 64 screen points and Ctrl+=/Ctrl+- zoom in and out by 20%.
Delete or Backspace removes the selected items. Ctrl-A selects every node while
the canvas has focus. Ctrl-Z and Ctrl-Y undo and redo through the diagram
session, including source edits. On Windows, Alt+Space remains available to the
native window system menu and does not activate the focused Diagram Studio
control.

Tab and Shift-Tab move between the source editor and other controls. Enter still
inserts a newline while the source editor has focus. In an open toolbar menu,
Tab/Shift-Tab and the arrow keys move among enabled items; Enter or Space
activates the focused item. Escape dismisses the menu and returns focus to its
toolbar button before it can close the Studio.

Use **New**, **Open…**, **Save**, **Save As…**, and **Close** in the toolbar.
Native Diagram Studio documents use the `.mld` extension and preserve the exact
source draft together with its graph layout and undo/redo history. Save on an
untitled document opens Save As, even when the document is blank. Open and Save
As start in the current document's directory when one is available, otherwise
in the application data directory.

Use **Import** to load a UTF-8 Mermaid `.mmd` draft. Unsupported text is kept
exactly and can still be exported as Mermaid source. **Templates** provides
Linear, Decision, and Process flowcharts plus starters for the ten
source-rendered families above. Importing or replacing any template in a
modified document uses the same Save, Discard, or Cancel decision as New/Open.
**Auto-layout** arranges a current native flowchart as one undoable edit.

The **Export** menu writes the exact Mermaid draft or a current image as SVG/PNG.
Image scale choices range from 0.5× to 2×. For source-rendered diagrams, PNG is
rasterized from the same accepted SVG used by the preview; the Mermaid source is
not evaluated again. Exports are separate files; they do not save or change the
active native `.mld` document. Export destinations must be unoccupied and cannot
be the active native document path.

Ctrl+N creates a document, Ctrl+O opens one, Ctrl+S saves, and Ctrl+Shift+S
opens Save As. New, Open, and Close ask whether to Save, Discard, or Cancel when
the document has unsaved changes. The same decision remains available when
closing with the window button, Escape, Ctrl+W, the launcher close command, or
unpinning the panel. A cancelled file chooser or failed save leaves the current
document and pending decision available. After an accepted Close, reopening
Diagram Studio starts with a fresh untitled document; use Open to return to a
saved document.

Drag the middle mouse button to pan, use the wheel to zoom, and choose **Fit**
to frame the current native graph or source-rendered image. **100%** uses the
renderer SVG's natural dimensions. **Reset view** returns to the default camera.
The Diagram Studio child window remains usable while the launcher window is
hidden.

Choose **Open viewer** in the toolbar to open a read-only **Diagram Viewer**
window, or **Focus viewer** to bring its existing window forward. It follows the
current document, showing the shared native graph or the current and stale
source-rendered preview. The viewer has its own camera: wheel, middle-button
drag, Ctrl+Arrow, and Ctrl+=/Ctrl+- affect only that view while it has focus.
Closing the viewer window leaves Diagram Studio and its draft open. Saving or
discarding an accepted Diagram Studio close also closes the viewer; Cancel keeps
both windows open. The viewer remains synchronized while the launcher is hidden.
