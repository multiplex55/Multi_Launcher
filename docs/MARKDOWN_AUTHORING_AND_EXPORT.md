# Shared Markdown authoring and export boundary

`notes_markdown` remains the shared owner for Markdown structure and source edits. Its authoring helpers keep byte ranges UTF-8 safe and return `PreparedMarkdownEdit` values bound to the complete source snapshot. Applying a prepared edit to changed Markdown returns a conflict instead of applying a stale range. Existing Note UI and checklist behavior are unchanged.

## Bounds

- Markdown tools accept source up to 1 MiB.
- Preview analysis accepts at most 65,536 physical lines and 1,000,000 parser events; over-limit previews retain the exact source and expose a limit status without presenting partial analysis.
- An oversized source is rejected before the preview copies or parses it; the caller's editable draft remains authoritative.
- A prepared snippet or individual Mermaid fence body is limited to 64 KiB.
- Preview recognizes at most 32 Mermaid fences and parses at most 1,000,000 CommonMark events.
- Mermaid preview bodies are limited to 512 KiB in aggregate.
- Prepared Mermaid SVG previews are limited to 2 MiB in aggregate.
- HTML output is limited to 8 MiB. At most 512 local images are queried; each resolver result is limited to 2 MiB and their aggregate to 4 MiB. Current Mermaid SVG embeds are limited to 2 MiB in aggregate.
- Local raster headers are inspected without full decoding; each side is limited to 8,192 pixels and the image to 16,777,216 pixels. Malformed or oversized images remain represented by their Markdown alt text.
- Table analysis is limited to 256 tables, 16,384 parser-reported cells, and 1,000,000 parser events. Row/column structural edits explicitly reject tables nested in blockquotes or lists; source-bound single-cell edits remain available there.
- Empty-selection quote and bullet snippets insert their starter prefix and leave the resulting edit selection at the insertion point.

## Mermaid fences

`MarkdownPreview` retains the full original source and records exact parser-derived byte ranges for each Mermaid fence and body. It asks the accepted Diagram parser and scene exporter for a current SVG. Invalid, unsupported, unclosed, or over-limit fences retain their exact source and carry a status; no partial Mermaid source is treated as a valid preview. Replacing a fence body produces a source-bound edit and ensures nonempty replacement text ends on its own line before the existing closing fence. If copied source could close the surrounding fence, that fence is lengthened without changing bytes outside the exact fence range.

## HTML safety

The static HTML exporter uses pulldown-cmark's HTML event writer. Raw HTML events become escaped visible text. Links are limited to HTTP(S), mailto, and same-document relative destinations after control/whitespace checks. Remote images are never loaded. An optional caller-provided resolver may supply bounded PNG, JPEG, GIF, or WebP bytes for a relative image reference; the exporter performs no filesystem or network access. Missing local images remain visible through their Markdown alt text and increment a warning count.

The document contains fixed CSS and a restrictive Content Security Policy, and adds no scripts, remote resources, embedded objects, or caller-supplied markup. Only current accepted Diagram SVG is embedded, each as a separate data image so generated SVG IDs cannot collide between fences. Unsupported Mermaid remains escaped source with a diagnostic. Markdown export returns the exact source bytes without newline normalization.

The Diagram source-copy request is an in-memory typed transition to a new untitled document. It uses the Diagram controller's existing dirty Save/Discard/Cancel policy and never writes a temporary file or changes the Markdown source.
