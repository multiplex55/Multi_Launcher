# Diagram Studio Mermaid flowchart subset

This document defines the source accepted by `diagram::parse_mermaid` and emitted by `diagram::format_mermaid`. It is an intentionally bounded flowchart subset, not a general Mermaid implementation. Parsing consumes each nonblank statement completely. Unsupported syntax produces a diagnostic; a valid prefix is never returned as if the whole file succeeded.

Syntax reference: [official Mermaid flowchart documentation](https://mermaid.js.org/syntax/flowchart.html). Only the forms listed below are admitted here.

## Grammar

Whitespace around tokens is allowed. Each statement occupies one physical line. A document starts with one header:

```text
flowchart TD
flowchart TB
flowchart LR
```

Blank source is an empty top-down graph. `TD`, `TB`, and `LR` are retained distinctly. `BT` and `RL` are recognized Mermaid directions but unsupported here.

Node identifiers begin with an ASCII letter or underscore and continue with ASCII letters, digits, underscores, or single hyphens. Consecutive hyphens are reserved for Mermaid edge syntax, and the exact lowercase ID `end` is reserved. IDs are case-sensitive. Node and subgraph IDs share one namespace, so their IDs must be distinct. A bare ID is a node reference; references create a default rectangular node when no declaration exists. Explicit declarations use one of these quoted forms:

```text
Start["Begin here"]
Wait("Rounded step")
Choice{"Continue?"}
```

These map to rectangle, rounded, and diamond shapes. Labels must be double-quoted. A bare node ID uses the ID itself as its default label. A later explicit declaration may supply a label and shape; two conflicting explicit declarations are invalid.

Connections are directed and may have a pipe-delimited label:

```text
Start --> Wait
Wait -->|ready| Choice
```

Edges are separate statements. Chains, undirected or bidirectional arrows, semicolons, and extra trailing tokens are unsupported.

Flat groups have explicit IDs and quoted labels:

```text
subgraph Decisions["Choose a path"]
  Choice{"Continue?"}
  Done["Finished"]
end
```

Subgraphs may not nest. Every opened group needs one `end`. A node belongs to the group where its first declaration creates it. A later bare reference does not move it between groups.

## Labels and entities

Quoted labels are read as Mermaid text, not JSON strings. Backslash has no quote-escape meaning. Encode a quote with `#quot;`; the parser also recognizes `#amp;`, `#lt;`, `#gt;`, `#apos;`, and decimal Unicode entities such as `#128161;`. Valid entities are decoded once. A literal hash that resembles an entity is emitted as `#35;`, so text such as `#quot;` remains literal after a format/parse roundtrip. `|` in an edge label is emitted as `#124;` so it cannot close the pipe-delimited label. Labels can contain Unicode, spaces, brackets, braces, parentheses, and hashes.

The formatter emits normalized quoted labels and LF line endings. This subset does not emit canvas positions or the domain's internal edge IDs; the native document preserves that metadata. Parsed edges receive deterministic IDs in connection order; the document session reconciles IDs across source edits. Mermaid formatting also groups members inside their explicit subgraph. Ordering that would require interleaving one group's members with unrelated nodes is normalized into a single group block. Nonempty group blocks follow their first member's node order; empty groups are emitted after node declarations.

## Unsupported and invalid source

The parser rejects unsupported constructs rather than ignoring them. Examples include comments and directives beginning with `%%`, styling/class/click statements, callbacks, nested groups, chains, semicolon-separated statements, other diagram kinds, unquoted shape labels, and unrecognized arrow forms. The source owner keeps the original draft intact and blocks graph-to-source mutations while the current text is invalid or unsupported.

Diagnostics carry a byte range into the original source plus one-based line and Unicode-scalar column. CRLF input is accepted; formatting normalizes it. Ranges always start and end at UTF-8 character boundaries.

## Bounds

The pure parser and canonical graph currently enforce:

- source: 65,536 bytes, 4,096 physical lines, and 8,192 bytes per line;
- identifiers: 128 ASCII bytes;
- decoded labels: 4,096 UTF-8 bytes;
- graph: 1,024 nodes, 2,048 edges, and 128 groups;
- positions: finite `f32` coordinates with absolute value at most 1,000,000.

The decoded label limit applies after one-pass entity decoding. A source document is accepted only if its normalized canonical form also fits the source, line, and line-count limits; this guarantees every successfully parsed graph can be formatted within those bounds. The formatter measures the actual encoded byte lengths before allocating output. Source and graph bounds are checked before constructing larger parse or graph collections. The versioned session codec preserves positions, edge IDs, source drafts, and revisions; Mermaid source alone is not a layout-preserving document format.

## Pure session behavior and native codec

`DiagramSession` owns the current source draft, its last-good graph/source snapshot, a monotonic revision and edge-ID high-water mark, bounded undo/redo, and a content-based saved baseline. A supported draft is `Current` only when it agrees semantically with the last-good graph. Invalid or unsupported in-bounds text is retained byte-for-byte and shown as `Stale` when an older graph exists, or `NoPreview` otherwise. Every visual graph transaction is blocked until the draft parses as supported current source. Parser line/count limits remain editable drafts; a source over the 65,536-byte limit is rejected atomically.

Supported text edits reconcile nodes by stable ID, retaining positions for survivors, including a survivor at the origin. New nodes receive deterministic anchors on a bounded 64-by-64 lattice with 180-unit spacing and integer coordinates from -32 through 31. Candidates are visited center-out, beginning at `(0, 0)`, so small new graphs stay near the origin. One shared cursor advances across newly seeded nodes, skipping any candidate less than 120 coordinate units from an occupied survivor or earlier new anchor. Existing source edges first match by directed endpoints and label, then by directed endpoints alone so relabeling retains identity. Each pass consumes parallel edges in source order. When parallel edges are indistinguishable, source order is the available stable tie-breaker. Unmatched edges use the session allocator, which is not restored by undo or redo.

Typed graph transactions are limited to 256 operations and validate operation labels/positions before cloning payloads. Each multi-node move is limited to 1,024 targets. Transactions publish only after graph validation and a format/parse semantic check. Failed operations and no-ops preserve source formatting, revision, allocator, history, and redo. Undo and redo restore source drafts (including invalid drafts), last-good graphs, layout, and edge identity; each successful history transition still advances the revision.

The version 1 JSON byte codec is an in-memory/native serialization boundary; it performs no filesystem IO. It preserves raw drafts, last-good snapshots, revision, edge allocator, and at most 64 combined undo/redo transitions. The exact serialized history entries are limited to 4 MiB total, and the complete encoded payload is limited to 8 MiB. Decode bounds source strings, graph collections, and history while deserializing, then validates every current and historical graph, supported source/graph correspondence, allocator high-water, history adjacency, current status, and payload version. Unsupported or invalid drafts remain lossless across a roundtrip. Decoding establishes the loaded current content as its clean saved baseline.

`DiagramDocumentController` keeps the active path and SHA-256 disk baseline outside the session history. Its explicit-path store reads at most 8 MiB plus one detection byte and accepts regular files only. Save As refuses an occupied destination; Save compares current bytes with the baseline before durable atomic replacement. This preflight detects changed or deleted bytes but is not a race-free compare-and-swap. The controller acknowledges the exact captured session content only after a successful write, while native bytes always include history, revision, and allocator state even when content is clean. A blank untitled document requires Save As. New, Open, and Close share one pending Save/Discard/Cancel decision; failed saves and failed pending Open leave the active session and decision available. A pending Open retains its path and rereads it when applied, including after saving the current document.

Saved-content comparison includes the exact draft and last-good source/graph, including node positions and native IDs, but excludes revision, undo/redo, and allocator bookkeeping. Returning to the saved content through undo becomes clean. A save acknowledgement applies to the exact captured snapshot; edits made while a save is in flight remain dirty when the older snapshot is acknowledged.

Scene construction and its renderer-independent geometry contract are documented in [Diagram scene geometry](DIAGRAM_SCENE_GEOMETRY.md).
