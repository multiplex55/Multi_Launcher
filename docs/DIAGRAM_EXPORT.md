# Diagram export

## Mermaid export

Mermaid export writes the exact draft as `.mmd`, or a current native flowchart
or source-rendered preview as SVG/PNG. Invalid or stale Mermaid can still be
exported as its exact draft. An image requires a current graph or render that
matches the active document revision.

Native flowchart SVG and PNG are rendered from the same bounded `DiagramScene`.
The caller chooses a finite positive scale; export rejects a result that exceeds 4,096
pixels on either side or 16,777,216 pixels total. It does not silently shrink
the requested scale. A blank current graph produces a valid 48-by-48 image.

Raster preparation is capped at 65,536 text glyphs and 128 million pixel
visits. Each fill is charged independently, including overlapping shapes and
the background. Glyph ink is measured from the bundled proportional font
family in its declared fallback order (Ubuntu Light, Noto Emoji, then
emoji-icon-font). The renderer keeps epaint's visual-only font scale separate
from layout advances, applies both vertical tweak terms in output pixels, fits
or clips ink to the scene's prepared text reservation, and substitutes a
visible square or question mark when no bundled face has the requested glyph.
The artifact reports the number of substituted glyphs. No system font search
is used.

PNG output is bounded to 80 MiB and SVG output to 8 MiB. SVG contains only
fixed local shapes, paths, clipping rectangles, and escaped text. XML control
characters are rejected; no script, foreign object, external reference, or
user-controlled markup is emitted.

Mermaid export refuses the active native path and checks for an occupied
destination immediately before the shared atomic writer. That check does
not make publication race-free. Export never changes the Diagram session,
native path, or saved content baseline.

## Visual Canvas image export

The rich Visual Canvas has a separate SVG/PNG encoder for its projected
scene, including supported rotated geometry, text, and embedded raster
images. It is not exposed in Studio's Export menu in this version. The
image options above apply to Mermaid documents.

## draw.io interchange

In a Mermaid document, choose **Import → Draw.io diagram…**. In a Visual
Canvas document, choose **Draw.io → Import diagram…**. Check the report, then
choose **Create .mld copy…** to write a new native visual document. The
original draw.io file and active draft stay intact. **Open copy…** uses the
usual Save, Discard, or Cancel prompt when the current document is modified.

To export a Visual Canvas document, choose **Draw.io → Export diagram…**,
review the current scene, then choose **Save .drawio…**. Use an unoccupied
destination. Unsupported content gives a refusal report and cannot publish
a partial file.

Use a single uncompressed `.drawio` page. XML is limited to 4 MiB. The subset covers direct layers,
rectangles, rounded rectangles, ellipses, diamonds, text, basic supported
styles, and labeled connectors with compass anchors. An imported label on
a shape can become independent text. The review lists inferred anchors.
Group and swimlane ownership, multiple or compressed pages, active HTML,
external resources, and unsupported geometry or styles are refused. Visual
exports also refuse embedded images, presets, shadows, and shapes outside
the supported subset.
Independent draw.io reader verification is still outstanding.
