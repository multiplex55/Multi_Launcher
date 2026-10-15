# M2-A bounded ShapePlan reuse prototype

Archive-only prototype in `target/egui-modernization/m2a/actual-layout-probe/`, derived from accepted05fc0950 plus frozen Stage2 attribution. No live source/Cargo change. FontFace owns at most eight plans, FIFO replacement, with actual HarfRust ShapePlanKey matching and field-split shaping borrows. No text/galley cache or second shaping engine. Plans drop with their face. Nine real preparation passes warm reusable plans; preparation counters are separate from measured owner work.

Source snapshots001–003 retain review/compiler feedback: corrected private-field/free-function access, in-place face ownership, occupancy on all-hit lookups, disabled clocks, non-Copy ShaperData borrow and explicit Default for38 counters. Standalone epaint tests could not resolve uncached mimalloc offline; root-package selection cannot test a non-workspace dependency's dev-dependencies. No fetch. The existing archive diagnostic boundary invokes the same key/FIFO/face isolation/replacement/variation assertions before preparation/timing; original package wrappers remain. Snapshot005 and this boundary independently source-accepted. Failed commands and malformed receipt004 retained.

Exact ignored `track_c_diagnostic_root_list_actual_layout_attribution`, locked/offline fast-dev Nextest:1 selected PASS,5236 filtered,native0,test134.62s,total289.941s including build. Four modes×100/1,000×cold/warm×five samples=80 records,two matching warmups,zero exclusions. Parent independently parsed all80 records, recomputed all16 medians/maxima and checked zero warm geometry/shaping, cold measured counts,25 painted rows and one pass. Final rustfmt passed. Full81 not repeated.

| Rows / state | Stock disabled p50 ms | Stock observed | Explicit per-call plan | Reused plans |
|---|---:|---:|---:|---:|
| 100 cold | 58.9784 | 59.0011 | 59.4827 | 32.8281 |
| 1,000 cold | 735.5561 | 740.5932 | 760.5017 | 385.8893 |
| 100 warm | 2.0781 | 2.0726 | 2.1433 | 2.2254 |
| 1,000 warm | 2.1518 | 2.0925 | 2.0691 | 2.2008 |

Reuse reduces cold median44.34%/47.54% versus disabled stock. Five ordered diagnostic samples do not establish warm parity; reuse warm medians are slightly higher and remain unresolved. Cold1,000-row p95 stock758.5911/reuse396.4917ms. All outliers retained, including explicit-plan1065.1864ms. This is material archive fast-dev evidence, not release/native acceptance or an M2-A performance pass. Original cold/warm gaps remain.

Measured1,000-row reuse work:4,047 lookups/hits,zero misses/builds/evictions,max2 entries per face; preparation4,150 lookups/4,147 hits/3 builds,max2. Lookup median3.433ms,remaining shaping211.552ms,segmentation27.3173ms,glyph emission60.8728ms. Nested inclusive times are not additive. Zero glyph-allocation cache misses,atlas recreation or galley eviction. Warm cases30 galley hits,zero shaping/rebuild/measured rows.

Untimed implicit/explicit/reuse comparison passes all4,047 ordered runs,103,830 glyphs and2,562 non-ASCII runs,stable actual font/asset identities,text/properties/features/variations and complete glyph arrays. All three hashes `2e54d32e325e40b8`; each owner independently passes eager geometry. Process-local face IDs differ as expected. Bahnschrift fixture hash `BFF1CA87EA35E376691498C618556411F0EE71784B32AA351E6E8F9E8BD609DD`,reported wght300–700/default400: supported-instance plan/key compatibility passed; full variable-font glyph parity was not tested.

Raw evidence prefix `receipts/shape-plan-shared-checks-owner-001`: command,native receipt,stderr arrays,format/provenance and preserved executable. Binary138833408 bytes SHA256 `943b3973896f7ed391d3d94eb47a485f158bded77167c17cc0277f4cd0833029`; stderr SHA256 `c66f7e265cf127c5c8de4050b23f05d15fbedc0966489cdc3d35a995eaa4af3b`. Complete six-file source provenance `shape-plan-source-final-001.txt` SHA256 `e4513341f834f44f4387be9951c9350cf42dabeb258dfa0a8310fade35b9d3ca`; parent verified all six current hashes. Frozen Stage2 tar copies preserve the prior source. Independent final result review accepted material archived savings/exact tested parity without findings. Next queued step is focused warm-row attribution; production integration requires a separate scoped packet.
