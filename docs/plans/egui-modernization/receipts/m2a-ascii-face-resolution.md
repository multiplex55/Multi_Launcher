# M2-A ASCII face-resolution storage candidate

Status: source and private harness independently accepted; no tests, ROOT build or timing body yet. Production796/Full ROOT FAIL unchanged. Plan9ecfb7d3; fresh ignored ascii-face-resolution-001/root copied from frozen796/4cf, separate from the warm guard copy.

CachedFamily owns exclusive ASCII [Option<FontFaceKey>;128] and non-ASCII map storage. Central helpers route existing Font::resolve_face reads/writes; ordered resolver/fallback, negativeSome(INVALID), both constructors, lifetime and reconstruction semantics remain intact. No shaping, segmentation, glyph allocation, layout, raw-result policy, dependencies or timers change. One private contract covers128ASCII cold/repeat against ordered lookup, exclusive containers, actual emoji fallback, empty negative caching and reconstruction. Actual fixed array-byte cost is pending execution; do not assume map savings.

Only fonts.rs SHA `1cde6124f28c1b813283c2fa85ace9ed29c1379073816746389703596dc84adb` and font.rs SHA `acc8b2ccaecbc9ac06a088a64001e51edf24ca2536f63667face14b04ee148bf` differ. Patch `50448df677999e90ef473c8751b56ab67e114d4ef671c51d11aa309723105784`; provenance `e88784715b34b461289fdcabd53f7b3a4a6007f84548fa6cf07c1e1f9c17c943`, under candidate receipts. Scopedformat0, CRLF preserved. Parent inspected exact patch and receipt hashes; reviewer accepted source/harness without findings.

Private harness manifest `ef241bb288e4d0e59aa5d232c4b1bc3076d4b7b01d2e498ae720169707bbc7b5` differs only in [lib].path pointing at this candidate; pinned lock `fc8fa3053c4abb2e57aa82f1e7ef579c17da56261222a5bd8764d69a4c441b1c` remains identical.

Next: exactly four targeted Nextest fast-dev library tests (new resolver contract; unchanged full_layout_matches_frozen_stock_output, positioned_row_measurement_matches_full_size_bits and test_fallback_glyph_width), fresh private target. Parent owns native execution. No ROOT/performance clearance or production adoption follows from private correctness.
