# M2-A exact size-only layout candidate

## Disposition

The source/private-contract gate is accepted after independent review. This is an ignored candidate experiment, not production adoption or performance acceptance. Production remains at the provisional paragraph-reserve implementation and the original/current Full ROOT performance comparison remains FAIL. Track B has not started.

Plan checkpoint: `aa64f407`. Candidate: `target/egui-modernization/m2a/size-only-layout-001/root`, copied from accepted `paragraph-glyph-capacity-001/root`. Immutable comparison archive remains SHA-256 `48b2b48ade003b3f534ab70b5a0e34724cf2aaa2189638df01328442c3d2a354`, executable `5d8703ac6e4e6140814370debd3f2a9738bbd02ce0c3588b7b94ba24625a1fbc`.

## Implemented boundary

Shared epaint shaping, wrapping, alignment, positioned rows and logical-rectangle rounding serve both full layout and exact measurement. Full layout retains complete meshes/metadata and the supplied job Arc. Measurement returns the exact logical size, never an incomplete Galley. Existing normalized full-cache hits preserve generation/child bookkeeping; ordinary measurement misses retain no second cache. Split-paragraph jobs use the existing full composition fallback.

WidgetText shares its existing preparation semantics (font override/fallback, color, line spacing, wrapping and vertical alignment). Existing Galley inputs pass through directly. Only `RootListGeometryCache::ensure` requests measurement; every absolute row remains measured. Visible rendering and the independent eager oracle remain full-layout consumers. Grid/Notes, cache policies and shaping algorithms are unchanged.

The root manifest adds only an egui path patch; the lock removes only that same egui 0.36.2 package's registry source/checksum. Reverse trees converge on one local egui/epaint 0.36.2 family. Parent locked/offline `cargo metadata --no-deps --format-version 1` checks returned native0 for both baseline and candidate: both workspace/default member selections contain only `multi_launcher` 0.1.0. The released egui crate checksum matches the accepted lock checksum; registry-copy comparison differs only in widget_text.rs.

## Independent stock output and corrections

The initial Debug fixtures were rejected because emath rounds Debug coordinates. They remain preserved as lossy history. Lossless fixtures were captured from a separate accepted-stock C115 copy, not candidate output. Review verified the stock runtime prefix/other Rust sources, identical fixture jobs/serializer, exact float bits, raw stored colors, glyph/row fields, meshes, texture IDs, bounds/counts and visual ranges.

Review identified and resolved four issues before release execution: preserve the supplied `Arc<LayoutJob>` rather than clone/reallocate; retain the cache lookup clock boundary before hit bookkeeping; retain ordinary full-miss `VacantEntry::insert` without a second lookup; explicitly exercise fallback-font resolution with overrides absent. No other defect was found in the reviewed affected paths. Existing trace/control assertions remain unchanged; no relaxation is justified before actual parity.

## Frozen bindings

| Candidate file | SHA-256 |
|---|---|
| vendor/epaint/src/text/text_layout.rs | `58031f55f8572d2c55451d4d2d3c0108c56f1c749c3e9e4ba5e1c1b6cb0b3883` |
| vendor/epaint/src/text/fonts.rs | `0ada3a172a5d74d0e24217cd674f4c6500dec3a0cb92de4c6db7587998eacacc` |
| vendor/epaint/src/text/text_layout_types.rs | `ca7d13db45d68f8821a635774ea772fa520adcf379c60d1e2f3eebf660d0b86e` |
| vendor/egui/src/widget_text.rs | `e0bb5152d41d112006e2fd264a4ef0b47d1ca669b50d55a32e29bbadaaaed88c` |
| src/gui/render.rs | `441810db2acaf507d6285e31b2dcdf59459e4a0a485d6fe323841921b67fa0bc` |
| Cargo.toml | `e3239bbca576b3bbe6d8648ff70590e0d4ad9fdd294735d8491a3ad0282fe2c3` |
| Cargo.lock | `18180d23d651f49d281e9c939df60e082f9d157d336401ca255eeba4187ec1c8` |

Accepted stock text_layout.rs: `c115272d7b7284d1805d4f8cbbeb31603a8da48d67e42a19b8cfac57329aef08`. Test-injected independent capture: `7a275471f5b75aacb87398057fd6db410df18758765b6f566548ed4a9e587bce`. Mixed fixture: `d75c7b2cc19ffd995fd5cef7e812a75dc3d9b3bee9adc2b8c85fac4a6fadd51d` (24,400 bytes); combining/emoji fixture: `3ab2a7ac0b9b1bffe09f45dbfed23f9c59e6d96323708ee8981eac3c35f5e2b7` (12,063 bytes).

## Actual focused verification and limits

Stable Rust1.99/MSVC, Nextest, offline resolution and isolated source target were used. Commands, selected filters, available tool-output excerpts, environment and limitations are retained in candidate `receipts/source-private-evidence-001.md`, SHA-256 `c0beb42c66656a98db945a5beac6e5ccb0ecc4359875eca60a54b350c8b6e9cb`.

- Corrected epaint stock-output/size/cache/Arc contracts: native0, five passed/54 skipped, Nextest `b1d00b70-e34f-4df5-9db9-a93a8119d0b4`.
- WidgetText preparation and existing-Galley pass-through: native0, two passed/50 skipped, `ea80d974-9c3b-4fdd-b887-3be07fe5854d`. Corrected fallback subcase: native0, one passed/51 skipped, `b2c70199-cde7-426b-af04-fd73a6e1503f`.
- Root eager geometry comparison: native0, one passed/5238 skipped, `0efb47b3-b947-4a44-a7aa-45afabd85fb2`; initial isolated app test compilation took6m31s.
- Selected combining/grapheme/cursor contracts: native0, three passed/55 skipped, `4fb06f5a-5fc8-404c-9e08-b34a2ade2bc8`.
- Independent stock capture and initial full-output comparator passed; their full original console output and exact CWD were unavailable after compaction. This limit is disclosed, not replaced with invented raw logs. Oracle source/independence was separately checked.

Most earlier execution evidence is transcript-derived, not redirected raw logs. Root/Unicode and original WidgetText pass-through results predate the final ownership/cache corrections; review accepted their reuse with the final focused corrected contracts, not a claim those earlier binaries included the later source.

One corrected epaint attempt has UNKNOWN native status after a console-encoding failure; one verifiable five-test invocation followed. A redundant corrected WidgetText logging invocation had already started when the parent stop message arrived: native0, one passed/51 skipped, `9d038a18-94ee-4284-99ce-d2c841d7f63f`. Its wrapper failed at fsync on a read-only handle after writing output/native status. Saved stderr SHA `3c6444263b4a4dff2c0d7a99570494c84769e0b0f726d4c615146868fa743b42` and native-exit file SHA `13bf7b3039c63bf5a50491fa3cfd8eb4e699d1ba1436315aef9cbe5711530354` were independently checked. No additional run was made to fix the wrapper.

No release archive, canonical parity body, performance/control body, GPU/native interaction qualification or production port has run for this candidate. No full suite was required for this bounded source gate.

## Next gate

After this local source-contract checkpoint, prepare exactly one canonical-release candidate artifact with frozen source/profile/executable and no-body listing review. Actual Trace parity review precedes the separately authorized matched baseline/candidate observer-control pair. Preserve existing assertions and all samples. Operation-only differences require actual classified evidence and review; no silent adapters or count changes. Production adoption and original/current Full ROOT qualification remain a later gate. Keep checkpoints local.
