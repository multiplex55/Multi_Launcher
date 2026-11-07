# Diagram Studio Mermaid flowchart subset

This document first defines the source accepted by `diagram::parse_mermaid` and emitted by `diagram::format_mermaid`. That parser is an intentionally bounded native flowchart subset, not a general Mermaid implementation. Parsing consumes each nonblank statement completely. Unsupported syntax produces a diagnostic; a valid prefix is never returned as if the whole file succeeded.

Syntax reference: [official Mermaid flowchart documentation](https://mermaid.js.org/syntax/flowchart.html). Only the forms listed below are admitted here.

## Source-rendered families

The ten families below use the separately bounded preview adapter. They remain
exact Mermaid source in the document session; the adapter does not create a
native flowchart graph, update native LastGood state, or expand
`diagram::parse_mermaid`. Source rendering is available only when the pinned
renderer strictly parses the whole source and the family's bounded line and
parsed-item accounting accepts it. This is a useful starter and preview subset,
not a claim of complete Mermaid compatibility.

| Family | Admitted source forms |
| --- | --- |
| Mindmap | One `mindmap` header followed by renderer-parsed node lines; source node accounting must match the parsed node count. |
| Sequence | `sequenceDiagram`, simple participant/actor declarations, and simple arrow messages with labels; parsed message count must match. |
| Class | Simple class identifiers, optional brace blocks with visibility-prefixed member lines (`+`, `-`, `#`, `~`), and one-line relation statements whose count matches parsed edges. |
| State | `stateDiagram` or `stateDiagram-v2` followed by transition statements containing `-->`; each statement must correspond to a parsed edge. |
| Entity Relationship | `erDiagram` relationship statements containing `--` and a relationship label after `:`; each statement must correspond to a parsed edge. |
| Gantt | Optional one title, nonempty sections and date format, and task statements; source task and section counts must match parsed data. |
| Timeline | Optional one title, sections, and `time : event` statements; event and section counts must match parsed data. |
| User Journey | Optional one title, sections, and task statements with a finite score from 0 through 5; task and section counts must match parsed items. |
| Pie | Optional one title, optional `showData`, and slices with finite positive numeric values; slice counts must match parsed data. |
| GitGraph | Basic commits, simple branch/checkout/switch commands, and merges with simple branch identifiers; commit and branch accounting must match parsed data. |

Common source bounds remain 65,536 bytes, 4,096 physical lines, and 8,192
bytes per line. Directives and frontmatter are not part of the admitted forms.
Other syntax may be rejected even when Mermaid itself accepts it. Do not infer
native graph editing, layout preservation, or full Mermaid parity from a
successful rendered preview. Native flowchart editing continues to use only
the grammar below.

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

Use **Suggestions** in Diagram Studio for keywords, snippets, and node IDs in
supported source contexts. A suggestion replaces only its indicated span;
one undo restores the exact previous draft. Unsupported source remains
available for editing and exact Mermaid export.

The native Mermaid parser supports flat subgraphs as described above. **Copy
as Visual…** is a separate operation and requires a current native flowchart
without groups. It creates a new Visual Canvas `.mld` and leaves the Mermaid
document and its exact source intact.

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

`DiagramSession` owns the current source draft, its native last-good graph/source snapshot, a monotonic revision and edge-ID high-water mark, bounded undo/redo, and a content-based saved baseline. A supported native flowchart draft is `Current` only when it agrees semantically with the last-good graph. Invalid or unsupported in-bounds native text is retained byte-for-byte and shown as `Stale` when an older native graph exists, or `NoPreview` otherwise. Generic renderer results have their own document/revision/options identity and never become native LastGood graph state. Every visual graph transaction is blocked until the draft parses as supported current flowchart source. Parser line/count limits remain editable drafts; a source over the 65,536-byte limit is rejected atomically.

Supported text edits reconcile nodes by stable ID, retaining positions for survivors, including a survivor at the origin. New nodes receive deterministic anchors on a bounded 64-by-64 lattice with 180-unit spacing and integer coordinates from -32 through 31. Candidates are visited center-out, beginning at `(0, 0)`, so small new graphs stay near the origin. One shared cursor advances across newly seeded nodes, skipping any candidate less than 120 coordinate units from an occupied survivor or earlier new anchor. Existing source edges first match by directed endpoints and label, then by directed endpoints alone so relabeling retains identity. Each pass consumes parallel edges in source order. When parallel edges are indistinguishable, source order is the available stable tie-breaker. Unmatched edges use the session allocator, which is not restored by undo or redo.

Typed graph transactions are limited to 256 operations and validate operation labels/positions before cloning payloads. Each multi-node move is limited to 1,024 targets. Transactions publish only after graph validation and a format/parse semantic check. Failed operations and no-ops preserve source formatting, revision, allocator, history, and redo. Undo and redo restore source drafts (including invalid drafts), last-good graphs, layout, and edge identity; each successful history transition still advances the revision.

The version 1 JSON byte codec is an in-memory/native serialization boundary; it performs no filesystem IO. It preserves raw drafts, last-good snapshots, revision, edge allocator, and at most 64 combined undo/redo transitions. The exact serialized history entries are limited to 4 MiB total, and the complete encoded payload is limited to 8 MiB. Decode bounds source strings, graph collections, and history while deserializing, then validates every current and historical graph, supported source/graph correspondence, allocator high-water, history adjacency, current status, and payload version. Unsupported or invalid drafts remain lossless across a roundtrip. Decoding establishes the loaded current content as its clean saved baseline.

`DiagramDocumentController` keeps the active path and SHA-256 disk baseline outside the session history. Its explicit-path store reads at most 8 MiB plus one detection byte and accepts regular files only. Save As refuses an occupied destination; Save compares current bytes with the baseline before durable atomic replacement. This preflight detects changed or deleted bytes but is not a race-free compare-and-swap. The controller acknowledges the exact captured session content only after a successful write, while native bytes always include history, revision, and allocator state even when content is clean. A blank untitled document requires Save As. New, Open, and Close share one pending Save/Discard/Cancel decision; failed saves and failed pending Open leave the active session and decision available. A pending Open retains its path and rereads it when applied, including after saving the current document.

The pure import boundary accepts an explicitly selected regular `.mmd` file, reads at most 65,536 bytes plus one detection byte, and requires UTF-8. It preserves the source string exactly, including a UTF-8 BOM, CRLF, comments, and unsupported syntax. Import creates a fresh untitled session; unsupported imported source has no native prior preview and remains editable as raw text. A separately accepted source-rendered family can show its matching generic preview without changing native LastGood state. If the current document is dirty, the controller keeps only the typed import path while awaiting Save/Discard/Cancel and rereads it when the decision is applied. The Linear, Decision, and Process templates use validated native flowchart source. The other ten templates provide bounded source-rendered starters and use the same fresh-session replacement behavior.

`layout_positions` is a deterministic pure initial-layout operation. It collapses cycles into stable strongly connected components, ranks connected components along the graph direction, places disconnected components at the first rank, and separates group members into lanes. Rank and lane spacing use the same node extents and group padding/header measurements as the shared scene builder. `DiagramDocumentController::auto_layout` publishes the resulting positions through one bounded `MoveNodes` transaction; IDs, labels, shapes, groups, direction, and edges remain unchanged. Repeating layout is a no-op; successful layout participates in the ordinary session undo/redo history. If scene geometry or coordinates exceed existing bounds, the operation fails without changing the session.

Saved-content comparison includes the exact draft and last-good source/graph, including node positions and native IDs, but excludes revision, undo/redo, and allocator bookkeeping. Returning to the saved content through undo becomes clean. A save acknowledgement applies to the exact captured snapshot; edits made while a save is in flight remain dirty when the older snapshot is acknowledged.

Scene construction and its renderer-independent geometry contract are documented in [Diagram scene geometry](DIAGRAM_SCENE_GEOMETRY.md).
