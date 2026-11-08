# Diagram Studio

Diagram Studio supports two document types. **Mermaid** keeps the source draft
and the editable flowchart or source-rendered preview it describes. **Visual
Canvas** keeps shapes, text, connectors, and their geometry in a separate
visual document. Use **New** (or **New Mermaid**) for a Mermaid document and
**New Visual** for a blank visual document. Opening a native `.mld` selects its
authoring type. **Source**, **Visual**, and **Split** are views of a Mermaid
document.

The [Mermaid subset reference](DIAGRAM_MERMAID_SUBSET.md) describes supported
syntax and source limits. The [export reference](DIAGRAM_EXPORT.md) describes
Mermaid image export and draw.io interchange.

## Mermaid authoring

A supported source change is parsed into the same graph used by the canvas.
If the source is invalid or contains unsupported native flowchart syntax, the
raw text stays available and the last valid native graph, when available,
is shown as stale. Graph changes remain disabled until the source is valid
again.

Mindmap, Sequence, Class, State, Entity Relationship, Gantt, Timeline, User
Journey, Pie, and GitGraph use the bounded source renderer. A current rendered
image appears in the preview area with Fit, zoom, and pan controls. These source
families do not create native graph nodes, an inspector, or Auto-layout
operations. If a new source revision is waiting for a render or fails, the
previous image remains visible with a stale indicator. The source stays
editable, and image export becomes available only for the render matching the
current document and source revision.

The Mermaid graph inspector lists nodes, edges, and groups. Select an item
there or on the canvas to edit its properties. Shift-click or Ctrl-click nodes to build a
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

Use **Import** to load a UTF-8 Mermaid `.mmd` draft. Unsupported text is kept
exactly and can still be exported as Mermaid source. **Suggestions** offers
syntax, snippets, and node IDs in supported source contexts. An accepted
suggestion changes only the indicated source span and is one undo step.
**Templates** provides Linear, Decision, and Process flowcharts plus starters for the ten
source-rendered families above. Importing or replacing any template in a
modified document uses the same Save, Discard, or Cancel decision as New/Open.
**Auto-layout** arranges a current native flowchart as one undoable edit.

The **Export** menu writes the exact Mermaid draft or a current image as SVG/PNG.
Image scale choices range from 0.5× to 2×. For source-rendered diagrams, PNG is
rasterized from the same accepted SVG used by the preview; the Mermaid source is
not evaluated again.

## Visual Canvas authoring

Use the shape picker and **Add shape** to place a primitive, and **Add text**
for independent notes or callouts. Double-click a primitive or its centered
label to edit its owned label. Enter commits, Shift+Enter adds a line, and
Escape cancels. The label moves, resizes, rotates, copies, and deletes with its
shape; existing free text stays independent. Labelled primitives cannot be
converted to presets because preset components cannot preserve their labels.
The font picker offers Sans serif and Monospace; an imported family is retained
until you choose a replacement.

**Add connector** creates a connection between
the first two eligible objects. Use the connector's properties to choose
endpoints, anchors, a route, arrow direction, and a label. Endpoints stay
attached as objects move. A route the canvas could not resolve is shown as a
fallback.

Select a connector to drag its route handles, use **Add bend** or
**Remove last bend**, or choose **Reset automatic**. Manual interior points and
curved controls stay at their world positions when an endpoint moves; the
endpoints remain attached. Copying and pasting a connected selection translates
its manual points with the copied objects. Curved controls and polyline bends
use their respective route types.

Drag or marquee-select on the canvas, and use the handles to resize or
rotate. A completed gesture is one undo step. Grid snapping, alignment
guides, Align, Distribute, layer and stack controls, and a minimap help
arrange the scene.

With the writable Visual Canvas focused, Ctrl+C, Ctrl+X, and Ctrl+V copy, cut,
and paste selected objects as a structured fragment; repeated pastes are offset
so they remain visible. Ctrl+A selects the canvas objects except the protected
document title; Delete or Backspace
deletes the selection, and Ctrl+Z, Ctrl+Y, or Ctrl+Shift+Z undo or redo. A
copied connector requires both endpoints in the selection. Text editing and
menus keep their own keyboard shortcuts.

Use **Add image…** to embed a bounded local PNG or JPEG. **Create preset from
selection** reuses a selection of supported primitives. Visual Canvas supports
containers, swimlanes and membership. Its nine visual starters are Flowchart,
Architecture, Mind map, Sequence-style, Dependencies, Organization Chart,
Network Topology, Decision Tree, and Swimlane Workflow. Replacing a modified
document with a starter offers Save, Discard, or Cancel.
The existing Mermaid templates remain available in Mermaid documents. Visual
Canvas also offers PNG and SVG export from its committed scene.
For supported visual documents, see [draw.io interchange](DIAGRAM_EXPORT.md#drawio-interchange).

## Native files and recent diagrams

Use **Open…**, **Save**, **Save As…**, and **Close** in the toolbar. Save on an
untitled document opens Save As, even when the document is blank. Open and Save
As start in the current document's directory when one is available, otherwise
in the application data directory.

Native `.mld` opens version 1 Mermaid, version 2 visual titles, and version 3
visual scenes. Mermaid saves stay at version 1 and preserve exact source,
graph layout, and undo/redo history. A version 2 title becomes editable text
when opened. A later visual save writes version 3, including scene
geometry, assets, and history. Opening an old document does not rewrite it.
A newer unsupported file is refused without truncation or rewrite.

Use **Recent diagrams** to find recently opened or saved diagrams by name or
path. It keeps a bounded local list of paths and does not search the
filesystem. **Duplicate…** copies the saved file, so save first to include
your latest changes. Use **Copy as Visual…** on a current native Mermaid
flowchart to create a separate `.mld` visual copy. The original source and
active Mermaid document stay intact. Flowcharts with groups and
source-rendered families are not eligible for this conversion.

Ctrl+N creates a document, Ctrl+O opens one, Ctrl+S saves, and Ctrl+Shift+S
opens Save As. New, Open, and Close ask whether to Save, Discard, or Cancel when
the document has unsaved changes. The same decision remains available when
closing with the window button, Escape, Ctrl+W, the launcher close command, or
unpinning the panel. A cancelled file chooser or failed save leaves the current
document and pending decision available. After an accepted Close, reopening
Diagram Studio starts with a fresh untitled document; use Open to return to a
saved document.

## Cameras and viewer

Drag the middle mouse button to pan, use the wheel to zoom, and choose **Fit**
to frame the current visual scene, native Mermaid graph, or source-rendered
image. For a source-rendered preview, **100%** uses the renderer SVG's
natural dimensions. **Reset view** returns to the default camera.
The Diagram Studio child window remains usable while the launcher window is
hidden.

Choose **Open viewer** in the toolbar to open a read-only **Diagram Viewer**
window, or **Focus viewer** to bring its existing window forward. It follows the
current document, showing the shared Visual Canvas scene, native Mermaid graph,
or current and stale source-rendered preview. The viewer has its own camera:
wheel, middle-button drag, Ctrl+Arrow, and Ctrl+=/Ctrl+- affect only that view
while it has focus.
Open and **Fit** make the visual viewer follow its available size; a manual
pan, zoom, or minimap move keeps that camera until Fit is chosen again. Drag
inside the minimap viewport to pan continuously, or click outside it to
recenter. Editor and viewer cameras are independent.
Closing the viewer window leaves Diagram Studio and its draft open. Saving or
discarding an accepted Diagram Studio close also closes the viewer; Cancel keeps
both windows open. The viewer remains synchronized while the launcher is hidden.
