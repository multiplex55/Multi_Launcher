# M2-A compact measurement materialization candidate

Status: six private correctness contracts, canonical artifact and ROOT comparison independently accepted as evidence. Adoption is held; production performance parity remains unproved. Plan `ad54f3aa`; candidate is frozen production796 in a separate ignored `compact-measurement-001/root`, excluding ASCII and live GUI guards.

Only runtime source change: `vendor/epaint/src/text/text_layout.rs`. A private compile-time LayoutGlyph interface chooses existing Glyph or compact MeasurementGlyph records (character/X/advance/line-height/section). Shared segmentation, shaping, emission, row-breaking, overflow, alignment and bounds remain one kernel. Private owned rows avoid public Row/Arc<Row>/RowVisuals materialization for measurement. Full finalization retains existing public glyphs/rows/visuals without an extra glyph copy. Measurement skips unused intrinsic metadata/per-glyph Y assignment; all allocate_glyph calls, physical-X snapping and atlas warming remain shared. Public APIs, Fonts/cache ownership, dependencies and ROOT callers stay unchanged.

Five existing contracts remain: full stock output, exact size bits, input-job Arc identity, grapheme glyph count and cursor roundtrip. The existing size test adds justified wrapped trailing whitespace and indentation/minimum-height empty-first-row cases. One new compact_measurement_preserves_cold_atlas_delta_and_size contract compares independent fresh Fonts states, equivalent initial atlas deltas, measured/full size bits and actual subsequent upload presence/position/dimensions/pixels. It also reports representation footprints. Six exact filters total; no matrix expansion.

Source review found two corrections: restore original x-(row_start_x+paragraph_min_x) floating-point grouping; require full_delta.is_some() to prove real cold warming. V1 source5e0dd41c/patchd4a7e59e/provenance8d12c54a retained. V2 source SHA `5049d463f60e63152119b05362902e2236ff97648b6540942457d33baaa47d05`; patch `dd82e4e61dc2e3c1b07eb7a501e013a40f7c685dfe38edb5529b63bb45579cf7`; source-preparation-v2.json `e4423c60587169675ad88e25b4d88311f42e69746d53c6845f68d526406dd8a1`. Scoped rustfmt0; parent normalized diff confirms exactly those two fixes. Independent focused re-review accepts corrected source.

Pinned copied private harness resolves only to candidate lib.rs. Manifest `e3deaef4b96eea3f19f8784a2aca585c8d1a94bb62fbfd0ddac80768d78d6e9c`; lock `fc8fa3053c4abb2e57aa82f1e7ef579c17da56261222a5bd8764d69a4c441b1c`, unchanged seed dependencies/features/profile. Canonical serialized fixtures remain exact Git bytes D75C/3AB2. Parent inventory:1207files in both roots, no additions/removals, only text_layout.rs plus those two documented fixture EOL restorations differ from copied frozen root.

Source ownership is released. Parent next executes the six exact private Nextest filters, locked/offline fast-dev lib harness, fresh private target. Compiler feedback will be corrected with failed evidence retained. No ROOT build/body or live source port is cleared. Actual production performance remainsFAIL; both mission tracks remain required.


First private execution: native101/26.8571363s (private-tests-001, Cargo29592), no test bodies ran. Inputf3559c4110325207e5b03610e4d8c890a7c0cac24be1671e2b9299d3a5439edf; stdoutempty/stderr1edea86079aac9cd5b8d3c4986fc74f2d95887a28cff45a1652ca78aa7c8d253. Compiler E0277: initial atlas-state assert_eq requires ImageDelta:Debug, which is absent. Correct only that assertion to assert!(exact_equal, message); retain exact PartialEq comparison, no image/public API change. Four pre-existing f32::INFINITY warnings in unchanged fonts.rs are outside scope. Source/harness/fixtures remained bound. V3 narrow test correction pending; reuse same target/six filters, preserve failed evidence. No correctness/performance pass.


Compiler correction v3 preserves the exact initial atlas comparison using assert!(lhs==rhs, literalmessage), avoiding the unavailable Debug bound. Parent V2→V3 diff confirms only this test assertion changed. Source `c2e5e0ebdd8ee4b982f324687cc22666f16f1d5202c2f30718aab92df261cfca`; patch `5302d3e8776a4bd0f86b775d859d95fefd2f3c1c87f11415cb14bca9385df9fd`; provenance `71adbc85fcd501fdc592bb665546b7ad507c0236dbd1acb6776b90b3285752cd`. Prior source/patch/provenance and failed native logs retained. Scopedformat0.

Same six filters, same private target: private-tests-002 native0/5.738786s, Nextest `4d4d3f15-660a-4501-9716-097578a36eee`, exactly6passed/54skipped. Input `f19f151d878996f94e2444b2c9afdb5a35662b32a1c743d86934b655a61472b0`; stdout `4a95c538d470e7dba4170050d0851fe347b5ce05cb8665361317b80a8d2af28b`; stderr `ce86d7440c9ad0a34bddb71a908f9ccec841c20d7b3a30d10cf0dc74c143b7e9`. Parent verifies exact names/count and source/harness/fixtures unchanged. Actual representations: full Glyph68bytes, MeasurementGlyph20bytes. Full lossless stock output, exact sizes including added geometry edges, job Arc identity, grapheme count/cursor, independent cold nonempty atlas upload and image/delta parity pass. This is private correctness/footprint evidence only; no runtime saving inferred. Independent completion review pending; planner preparing one bounded existing-protocol ROOT comparison packet without activating source/build/body. Production FullFAIL/fullmission remain.

Independent completed-source/private review accepted without findings: V3 equality semantics, all six contracts, nonempty cold upload/size/delta/image parity, canonical full visuals/jobidentity/cursors and68B/20B footprints verified. Firstcompile101 retained. Source/private milestone complete only; no performance/adoption clearance.


Reviewed compact ROOT experiment plan916e02dc is saved locally. Parent canonical artifact preparation actually started2026-10-14T16:52:46.291447Z, Cargo9040, owned session74037, input `622c8b2930b48103419721ec893f05c6814b3072489def9720697c56b4aa5ffa`; evidence `compact-measurement-001/receipts/root-artifact-001`. No active compiler observed before start. Reused accepted canonical target; only candidate root/epaint library timestamps invalidated, no cleanup attempted. Source/pins/fixtures/tools/font assets and preserved F26D/C60C/A438/ASCII29F5 hashes checked. Parent retains completion handle; silence is not completion. Require actual epaint/root fresh=false/correct paths/canonical profile and exactly one no-body Full-owner listing, then independent artifact review before any body.

Interpretation is bounded: ROOT comparison measures compact measurement and warmROOT overhead only. Nine preparation frames warm visible full galleys, so cold full-Galley finalization performance remains unresolved. No performance/adoption/GUIguard qualification or TrackB clearance.


Canonical compact build completed native0/454.1563378s; result `939333cf102f0c309480e66a8d9afd302baed93c597b5f33abb25c74136a1906`, rawstdout99886C42/stderr881E5ECD. CargoJSON confirms epaint/root fresh=false at correct candidate paths, root canonical release/defaultfeatures. Preserved actual compact ROOT libtest `6e7a84f537e64cf120c8f6eb262eb28e46982f4e9c446763b20718d5d542f02b`,86,830,592bytes; selectionf78330382d76ec111db2a0c2d12023ea9389beadab6e794c8300de9865804c51. Source/pins/fixtures/tools/fonts and all four preserved executable copies remain exact. Exactly one Full-owner no-body listing native0, resultf54c33cd63fcc0620275c534df41e53de051493974dc8780e25194a1db610bd8; no timing body ran. Independent completed artifact/listing review pending. Prepared direct pair runner binds the exact completed gate and executable; activation still awaits review/local checkpoint.

Compact canonical artifact/listing independently ACCEPTED withoutfindings: actual6E7A84F5/86830592, native0/454.156s, freshcandidateepaint/root/canonicalprofile/defaultfeatures, bytepreservingtimestampinvalidation/source/pins/fontassets/copies and one exactFullowner no-bodylistingverified. Localartifactcheckpoint precedes separatelyactivatedsingleF26baseline→compactpair. No adoption/productionqualification.


## Single contemporaneous compact Full ROOT comparison

After artifact checkpoint7e2a19b4, parent ran exactly one F26D/source796 baseline body then one compact6E7/source796+patch5302 body. Native exits0/0; wall37.6483858s/35.1921797s. No source/compiler/benchmark concurrency, retry or exclusion. Direct evidence in compact-measurement-001/body-runs/full-baseline-001 and full-candidate-001. Offline index `matched-full-compact-001.json` SHA `7f30fc1f8fc4fd7fab757f2e91e1b79a912d65bd3ff487af6f82757bc814a7b1` retains all12records/240timings/seven aligned vectors. Exact cross-arm output/structural/revision/viewport/work identities and within-case stability pass; cold one rebuild/allNmeasured, warm zero geometry work,25paint. Source/executables unchanged.

Milliseconds; p50/p95 from sorted copies at indices9/18, raw order retained.

|Rows/state|Baseline p50/p95|Compact p50/p95|
|---|---|---|
|100 cold|1.2887 / 1.7121|1.0165 / 1.2614|
|100 warm|0.2721 / 0.3505|0.2457 / 0.2997|
|1,000 cold|15.8193 / 18.3386|13.5169 / 16.5348|
|1,000 warm|0.2927 / 0.3808|0.2960 / 0.4934|
|10,000 cold|161.3453 / 176.8390|135.6845 / 151.3537|
|10,000 warm|0.3097 / 0.3875|0.3234 / 0.4079|

Independent actual-evidence review accepted without findings: bindings/protocol/allvectors/quantiles/output/work verified. Disposition: useful descriptive cold signal (~14.6%1k/~15.9%10k), HOLD adoption. At10k candidatep95 is below baseline minimum157.0224ms, but full ranges still overlap (candidate max165.9882ms); retain baseline max281.6345ms without exclusion. Warm results are mixed:100improves,1k median nearlyflat but tailhigher,10k modestlyhigher. One fixed baseline-first pair/host variation cannot establish causal savings or a reproducible warm regression. No original-parity or production pass follows. Cold full-Galley finalization remains unqualified; existing Grid/Notes later evidence must prove actual fresh full work. Planner investigates the narrowest existing-owner follow-up using alreadybuilt artifacts; no newtimer/build/body/sourceport activated. ActualproductionFullFAIL and both mission tracks remain required.
