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

## Completed canonical-release artifact gate

The single canonical archive build completed native0 in728.0730448s; the fresh full no-body listing completed native0 in2.3381692s. Archive SHA-256 `f62ca5c635234dd1796d691faa2f4818bcbedd64b8f6fe24dae6cfabe2c2a4a4` (30,635,445 bytes). Built/extracted executable SHA-256 `ebbb2dc3ba820ff4780a64a8cbc370bd33211d4f1c8f3e70c12cf7184951663f` (87,190,528 bytes); extracted path is `size-only-layout-001/extract-canonical-release-listing-001/target/release/deps/multi_launcher-4b74913c56898d7e.exe`.

The listing contains5,239 multi_launcher test cases. Both exact ignored owners, `track_c_diagnostic_root_list_raw_hit_precedes_shaper_setup` and `track_c_diagnostic_root_list_release_observer_control`, are present; full-list mismatch reason `ignored` is expected. No test body ran. All nine frozen source bindings and baseline archive preservation were rechecked. Parent independently verified127 unique linked file bindings, the x86_64 PE header, exact listing owners/count, source/version/environment closure and native result records. Independent artifact review accepted without findings.

Receipts are in the outer candidate `receipts` directory: input SHA `4c49dc24950f8f54d0cebba106e2ab98e2677bcc06060abd8448a9f552f9d86d`, runner `be1512cc0ae15f64591b6d05e6b8d6596b3a87bacdbfdda5114c743f15b1a70c`, build result `c28a92353a5a36bfe329fad7d5fc7246d6d02a8cb466d79014b2ed85e0337eb5`, list result `a1b827785917d3a0141260fdbe1aa66b12d62536a421799f14a8e398a6d7ac05`. Direct native logs are retained. A Python3.11 TOML parsing incompatibility was corrected in the ignored runner before launch; no build/body was consumed by that preflight failure.

Default-font asset supplement `bcca2955e6d6e26c071671574144b730a899832de0bf24578574c0b598c1d609` is explicitly observed during the live build, not pre-launch; all four assets and cached crate bytes match the accepted baseline. Immutable input was not rewritten. Parent independently observed the owned root rustc36512 during build with `--test`, `-C opt-level=3`, `-C strip=debuginfo`; canonical release/MSVC/default-feature/override-free provenance was accepted. This compile evidence is not parity or performance acceptance.

Actual candidate Trace parity is the next separate gate. No timing/control/production body has run for this candidate.

## Actual candidate Trace parity gate

The single candidate parity-only invocation passed native0 in2.3192638s (Nextest556b9174-5e7e-4397-8651-23354719d738; one passed/5238 skipped; summary0.210s). The complete4,964,373-byte snapshot is byte-identical84BDA and the complete8,632-byte classified diff is byte-identical075EF/status=pass. Existing assertions and cross-profile adapter were unchanged.

Actual work is one rebuild,1,000 measured rows,25 painted rows, one adapter pass,4,047 ordered runs/face runs and103,830 glyphs. Plans975/975/0, native shape calls975, rawlookups/hits/misses4047/3072/975. All55 diagnostic fields are retained; parent independently compared all41 non-timing fields to the accepted prior parity output and found exact equality. Release identities remain `adfb11ee80438eab` / `18c853161fe60dfc` / `89195c4b0382794f`. Parity timing fields are not a performance series.

Parent verified29 unique linked file bindings and both full byte comparisons. Independent parity review accepted without findings. ArchiveF62C/exeEBBB, final source bindings, separately unchanged `text/font.rs` EBE (distinct from modified `text/fonts.rs`0ADA), and reference66D before/after are unchanged; fresh extracted executable matches. Start receipt SHA `1eb4e3f923f1b45122f3eef587e9bc7a605ba904447f6b15cd567c766a3163a3`, result `aa05e68c07343f266fca32268c9e2c123c1ab10037160431fafc58794981d721`, direct stderr `bb05276a41d3fec3092e6657c2b194ed62501d466fc958bbec12bfb5fa09d2e5`. Complete files are retained in `size-only-layout-001/raw-hit-parity-only-output-001`.

No warmup/timing/control series, baseline/original body, production port or performance waiver followed this parity-only invocation. The next separate gate is one fresh matched baseline/candidate existing observer-control pair, with all samples retained.

## Matched control evidence and experiment disposition

The bounded experiment is complete and independently accepted. Exactly one fresh reserved-capacity baseline body then one size-only candidate body ran: native0/4.2362672s and native0/4.1772109s; each one owner passed/5238 skipped,14 records. No explicit Nextest UUID line was emitted; actual commands, PIDs, native summaries and direct logs are retained rather than inventing IDs. There were no repetitions, exclusions, source changes or rebuilds in this pair.

| Observer state | Baseline owner p50/p95 ms | Candidate owner p50/p95 ms | Interpretation |
|---|---:|---:|---|
| Disabled | 25.3959 / 34.2486 | 16.4733 / 19.6152 | Primary benefit signal |
| Counters enabled | 31.6263 / 33.5289 | 16.3410 / 25.7514 | Supporting instrumented data only |

Disabled ordered samples: baseline `[34.2486,24.9338,25.7314,25.3959,24.4816]` ms; candidate `[16.4733,17.3381,19.6152,15.4585,15.2652]`. Ranges24.4816–34.2486 vs15.2652–19.6152ms are disjoint; all five corresponding candidate samples are lower. Enabled ordered samples: baseline `[33.5289,32.7177,26.5378,31.6263,25.4895]`; candidate `[16.3410,25.7514,18.5684,16.2942,16.0595]`. Nearest-rank p50/p95 for five measured values are the third/fifth sorted values; all samples, including the baseline34.2486ms value, remain included.

All28 raw records (eight warmups/twenty measured samples), all55 diagnostic vectors, state order/protocol and ordered arrays were retained and independently checked. Work `[1,1000,25]`, revisions `[2,1,1000]`, one adapter/geometry call/completion, zero error/abandoned outcomes, eager/typed-ID checks, identities and all41 non-NS diagnostic fields match across arms. Disabled vectors are allzero; enabled count/storage invariants pass. Parent/reviewer independently verified50 unique actual file bindings and raw-log hashes. Internal controls remain counterbalanced as in the existing owner; arm order remains fixed baseline first.

Arm result SHAs: baseline `afc8044dbee0b7f56a591787811e93c5a8bd0962e330cdbcf63363324d49b6c5`; candidate `ebb031b34615275f4ddef0b9f46249c533ffe963f12e9ec93f04ac3dd26e0c71`; matched review `f3d7dfd90332398955e0346f6f4a9bec5c2d1d78a346b79ac5c64eaba9771f69`. Direct stderr SHAs: baseline `3a5d9daf78f2c98ba01f9bce0dad16711a5eb3346f18376fc1d5f412d2685916`; candidate `7c957f3f9f3aff94e3487203fe6e5f0586673e2a1610f97804e94007ad172e2c`. These files remain in outer candidate `receipts`.

Append-only clarification `5ef0c7ee8a200c22e70acb09ac0dcb6ee6edece88f4afac97c9d76e20394dd94` preserves the original matched index and corrects its clock-field label: ATLAS_FILL_BEFORE/AFTER_MICROS are atlas state measurements, not elapsed clocks. Both fields match at9277 in every enabled record and0 in disabled records; they were included in parent non-NS parity checks. No rerun or parity gap resulted.

This single fixed-arm-order pair is descriptive, not a causal percentage or production performance qualification. Nested enabled clocks are nonadditive and are not subtracted from production medians. Review supports a separately reviewed provisional production port followed by actual original/current Full ROOT qualification. Live source remains unported, production Full performance remains FAIL, and Track B is still gated. A read-only planner is preparing that bounded next packet; no live write or extra benchmark is authorized by this evidence report.
