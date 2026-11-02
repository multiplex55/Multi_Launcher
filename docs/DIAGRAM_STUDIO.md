# Diagram Studio

Diagram Studio edits the accepted Mermaid flowchart subset through either its
source editor or graph canvas. The [subset reference](DIAGRAM_MERMAID_SUBSET.md)
lists the exact syntax and bounds.

Use **Source**, **Visual**, or **Split** to choose an editing view. A supported
source change is parsed into the same graph used by the canvas. If the source
is invalid or contains unsupported Mermaid, the raw text stays available and
the last valid graph is shown as a stale preview; graph changes remain disabled
until the source is valid again.

The visual inspector lists nodes, edges, and groups. Select an item there or on
the canvas to edit its properties. Shift-click or Ctrl-click nodes to build a
multi-selection. Drag selected nodes together, or use the arrow keys to move
them; Shift-arrow uses a larger move. Delete or Backspace removes the selected
items. Ctrl-A selects every node while the canvas has focus. Ctrl-Z and Ctrl-Y
undo and redo through the diagram session, including source edits.

Drag the middle mouse button to pan, use the wheel to zoom, and choose **Fit**
to frame the current graph or **Reset view** to return to the default camera.
The Diagram Studio child window remains usable while the launcher window is
hidden.
