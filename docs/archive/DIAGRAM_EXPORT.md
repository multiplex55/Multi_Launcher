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

Mermaid export refuses the active native path. Image and source artifacts use
a synced same-directory stage and atomic no-clobber publication, including
refusal when a destination appears after staging. Export never changes the
Diagram session, native path, or saved content baseline.

## Visual Canvas image export

In a Visual Canvas document, open **Visual export** to choose scale and a
transparent, white, or custom RGBA background, then choose **Export SVG…** or
**Export PNG…**. The current committed scene is captured independently of
Viewer pan and zoom, including supported rotated geometry, owned shape labels,
manual connector routes, text, frame headers, shadows, and embedded raster
images. The editor, viewer, and image exports use the same projected route.

Encoding and staging run in the background. A current accepted owner pass
publishes with the same atomic no-clobber protection. **Cancel export** is
available while work or an unpublished stage is cancellable; another export
waits until cancellation settles. Closing or replacing the document retires
stale work. The native document, saved baseline, and Viewer camera stay intact.

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

Use a single uncompressed `.drawio` page. XML is limited to 4 MiB. The subset
includes layers, nested transparent groups, compatible containers and horizontal
or vertical swimlanes, rectangles, rounded rectangles, ellipses, diamonds, text,
basic styles, and labeled connectors with compass anchors. Supported primitive
vertex labels become owned shape labels; group labels and independently authored
text remain separate. Frame headers must match
the native header geometry, with a transparent body and matching text and
border colors. The inert `mlFrame=container` style preserves native Container
kind when standard swimlane styling is used; inconsistent markers are refused.
The header extent must be `min(28, 0.28 × height)` for a
horizontal frame or `min(28, 0.22 × width)` for a vertical frame. A rotated
leaf is supported; a rotated parent with descendants is refused. Parent-local
coordinates must resolve within native geometry bounds.

The review identifies unsupported content, inferred anchors, and connector
routing parents that will normalize to the endpoints' common frame. Cross-layer
membership or connectors, frame stacking that cannot be represented by a tree, relative
vertex ports, multiple or compressed pages, active
HTML, external resources, and other unsupported geometry or styles are refused.
Straight-style polyline waypoints can round-trip when their coordinates are
representable exactly. Manual curved controls, incompatible waypoint routing
styles, and waypoint coordinates that cannot be represented without precision
loss are refused rather than flattened. Export also refuses empty owned labels
and owned-label opacity or alpha combinations the draw.io subset cannot preserve.
Visual exports also refuse embedded images, presets, shadows, and shapes outside
the supported subset. No partial file is published on refusal.
Independent draw.io reader verification is still outstanding.
