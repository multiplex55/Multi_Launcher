# Markdown Studio

Markdown Studio edits plain UTF-8 `.md` files. Search for **`markdown studio`** or open it from the launcher. The editor keeps the original text, including a UTF-8 BOM and CRLF line endings, and saves with an atomic write. Open, New, and Close ask whether to Save, Discard, or Cancel when the document has unsaved changes. Saving an untitled document opens Save As; Save As does not overwrite an existing file.

## Editing

Use **Edit**, **Preview**, and **Split** to choose a view. The Format and Insert menus apply source-bound formatting, snippets, and tables to the current selection. When the caret is inside a table, the Table menu offers row, column, and cell operations. These actions and keyboard undo/redo use one document history; stale selections are rejected instead of changing a different part of the document.

The outline follows Markdown headings and can select their source text. The preview renders the full CommonMark document, including nested lists, quotes, tables, and inline formatting. Mermaid fences also receive separate Diagram preview cards. Invalid or unsupported Mermaid remains visible as source with its diagnostic.

Local images are previewed only when the document has a saved location and the image is a valid relative file beneath that document's directory. Remote, absolute, escaping, missing, malformed, and oversized images are not fetched; their alt text remains visible. Preview does not make network requests. Explicit HTTP(S) and `mailto:` links can be opened by clicking; unsafe schemes and control-obfuscated destinations are blocked.

## Mermaid diagrams

Choose **Open Diagram** beneath a Mermaid fence to edit that block in Diagram Studio. The Diagram uses the same dirty-document Save/Discard/Cancel flow as a manually opened diagram. When the Diagram still belongs to that exact Markdown source and fence, **Use source in Markdown** replaces only that fence body as one undoable edit. If the Markdown has changed or the Diagram was replaced, the handoff is rejected rather than guessing which block to edit.

## Export and bounds

The Export menu writes either the exact Markdown source or a static HTML document. HTML escapes raw HTML, uses a restrictive Content Security Policy, does not load remote images or scripts, and includes only current Mermaid previews. Local image files are embedded only after the same relative-path and size checks. Export refuses occupied destinations and the active Markdown file.

Markdown documents are limited to 1 MiB, 65,536 physical lines, and one million parser events for preview analysis. Mermaid preview handles at most 32 fences, 64 KiB per body, 512 KiB of fence source, and 2 MiB of generated SVG. HTML output is limited to 8 MiB. Over-limit source remains editable and export reports the limit rather than displaying a partial preview.
