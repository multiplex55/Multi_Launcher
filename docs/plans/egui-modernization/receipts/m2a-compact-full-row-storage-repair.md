# M2-A restore full row-storage ownership

Prerequisite: accepted private diagnostic checkpoint `65986dfe` confirms retained wider outer-row backing on one fast-dev fixture, without establishing a latency cause. Existing compact ROOT/Grid adoption remains HOLD.

Planner recommendation: restore the original full-output storage boundary rather than shrink or allocate a replacement public vector after conversion. Full layout constructs and carries `Vec<PlacedRow>` directly; measurement retains compact glyphs/private owned rows. Both retain one geometry kernel. Shrinking or adding a second vector leaves the conversion traversal and adds allocator work.

Implementation is confined to a fresh ignored `compact-full-row-storage-001/root`, copied from frozen compact C2E5, with edits only in `vendor/epaint/src/text/text_layout.rs` and its private tests. Frozen C2E5/6E7, diagnostic BBB42, live6527 and unrelated workspace files stay unchanged.

Required steps:

1. Extend the existing private compile-time distinction with row representation: full output uses actual `PlacedRow`; measurement uses compact owned rows.
2. Shared `rows_from_paragraphs`/`line_break` construct the selected representation. Full mode creates `Arc<Row>` at the original source796 stage and moves glyph vectors into it.
3. Shared overflow/alignment/positioning use a narrow private row-access boundary. Obtain mutable full-row access through `Arc::make_mut` once per row operation, never per glyph. Keep one geometry algorithm and avoid a general dispatch framework/public API.
4. Carry the selected vector through `PositionedLayout`. Full finalization consumes the existing public vector directly; remove full `LayoutRow<Glyph>` map/collect conversion. No shrinking, replacement vector, glyph copying or allocator policy.
5. Preserve compact measurement and intrinsic/Y-output distinction, exact floating-point grouping, shaping/atlas warming, wrapping/indentation/whitespace/justification/truncation, section selection, clusters/cursors, UV/color/mesh output, job Arc identity and caches. No font/dependency/renderer/cache-policy change.

Source preparation first: fresh candidate-bound copy of the pinned private harness/lock, canonical fixtures, scoped formatting, bounded diff/provenance and released source ownership. Parent inspects and obtains independent source review before private execution. Agent performs no build/test/native execution.

Proposed narrow verification: strengthen the existing cold-atlas contract on its same fixture with a statically typed `Vec<PlacedRow>` boundary, preserved outer allocation/capacity/length through finalization, preserved glyph-vector storage, exact serialized full equality and observational outer bytes. Preserve all original assertions. Run the same six accepted exact private Nextest filters, locked/offline fast-dev, reusing the released compact target. No seventh test/fixture matrix or additional performance owner. Reviewer accepted this plan without corrections; extending the existing contract keeps the budget at six filters.

Done criteria: full path has no wider intermediate row vector/conversion pass; compact measurement remains; storage and all six parity contracts pass; scoped diff/source binding independently reviewed. Timing effect remains unknown. No ROOT/Grid repeat, release performance claim, production port or Track B clearance is part of this packet.
