# Diagram export

Diagram image export is captured from one `DiagramSession` revision and its
current graph. A stale or unsupported Mermaid draft cannot be turned into an
image. Mermaid source export is separate and always writes the exact current
draft, including unsupported text and line endings.

SVG and PNG are rendered from the same bounded `DiagramScene`. The caller
chooses a finite positive scale; export rejects a result that exceeds 4,096
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

Publication is explicit and refuses an occupied destination immediately
before the shared atomic writer. The check is not a race-free filesystem CAS.
Export publication never changes the Diagram session, native path, or saved
content baseline.
