# Diagram scene geometry

`diagram::build_scene` prepares bounded, renderer-independent geometry from a validated `DiagramDocument`. It keeps model IDs on every node, group, and edge. Node positions are centers in diagram coordinate units. The scene is a derived view: it does not change the graph, choose a camera, perform automatic layout, or hold selection and drag state.

## Text and node bounds

`SceneTextMetrics` makes text layout deterministic without depending on a font or GUI library:

- Each Unicode scalar reserves the configured `max_scalar_advance`.
- Explicit `\n` characters start a new line.
- Longer lines hard-wrap at Unicode scalar boundaries to fit `wrap_width`; words are not reflowed or removed.
- Each prepared line has the configured `line_height`.

The default text metrics reserve 8 units per scalar, 16 units per line, and a 320-unit wrap width. `PreparedTextLine` carries the original line text, scalar count, and reserved width; `PreparedText` carries the maximum line width and total height. A centered line starts at `label_bounds.center.x - line.reserved_width / 2`; a leading-aligned line starts at `label_bounds.min.x`. Scalar `i` occupies `[line_origin_x + i * max_scalar_advance, line_origin_x + (i + 1) * max_scalar_advance]`. Line `i` occupies the row beginning at `label_bounds.min.y + i * line_height`. Node and edge text is horizontally centered; group titles are leading-aligned. The width is an explicit reservation, not a measurement of any font. A renderer must fit shaped glyphs within each line's reserved width or clip them to the prepared label bounds. It should configure `max_scalar_advance` conservatively for the glyphs it renders. This contract prevents CJK, emoji, combining sequences, or a fallback font from silently overflowing scene bounds without requiring a font engine in the pure scene module.

Node labels are centered in their shape bounds. Rectangle and rounded-rectangle sizes include horizontal and vertical padding and configured minimum dimensions. Diamonds expand until the rectangular label bounds fit inside the diamond.

Rounded rectangles use a corner radius clamped to the node's half-width and half-height. Ray intersections use bounded bisection. Rectangle and diamond intersections use their respective axis-aligned boundary equations.

## Groups and edges

A nonempty flat group bounds the union of its member nodes, padding, and a title header above them. An empty group gets a title-only box after the node extent; empty groups follow document order. The first empty group starts at the origin when there are no nodes.

Every directed edge is a cubic Bézier with start and end points on the source and target shapes. Its arrowhead tip is the end point and its base follows the final curve tangent. Edge labels are centered near the curve midpoint, offset by the configured label gap. Parallel edges sharing ordered endpoints are sorted by stable `EdgeId` and receive centered, evenly spaced lanes. Self-edges use outward cubic loops. Their control-point clearance is based on the node's larger half-extent plus self-edge spacing, increasing by parallel spacing in stable edge-ID order; the loop therefore grows with large nodes as well as with parallel self-edges. Distinct nodes with coincident centers use deterministic outward arcs. Their clearance is based on the larger endpoint half-extent plus self-edge spacing, increasing by lane distance for parallel edges, so even the single-edge lane clears both shapes.

The optional total bounds conservatively contain node and group rectangles, Bézier control hulls, arrowheads, and edge labels. An entirely empty scene has no total bounds. Public geometry uses `f64`; source graph coordinates remain the model's validated `f32` values.

## Bounds

The model limits scenes to 1,024 nodes, 2,048 edges, 128 groups, 4,096 UTF-8 bytes per label, and positions with absolute coordinates no greater than 1,000,000. Metric ranges are finite and checked before building geometry:

- maximum scalar advance: `[0.01, 256]`; line height: `[0.01, 1,024]`; wrap width: `[max_scalar_advance, 16,384]`;
- node padding and group padding/header gap: `[0, 8,192]`;
- minimum node width/height: `[0.01, 32,768]`; rounded corner radius: `[0, 4,096]`;
- parallel spacing: `[0.01, 8,192]`; self-edge spacing: `[0.01, 8,192]`; edge-label gap: `[0, 8,192]`;
- arrowhead length and half-width: `[0.01, 1,024]`.

Total generated coordinates may not exceed 1,000,000,000 in absolute value. Invalid graphs, metrics, or extents return a `SceneError` before a scene is published.
