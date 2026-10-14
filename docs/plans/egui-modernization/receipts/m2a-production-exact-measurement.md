# M2-A production exact measurement

Status: source, focused tests, canonical release artifact and actual measurement evidence independently accepted. **Production Full ROOT performance remains FAIL.** The source port is provisional; M2-A, the native/backend gates and both migration/theme tracks remain open.

## Implemented and reviewed

Local source commit: `79653eca51c80c8777b1b7e9ddaf6fd9a826e823`, tree `4cf8646d17f7ec2a3d0ebf62e7c2e46df5305bba`. Artifact evidence checkpoint: `3c16ee845479703976a2def0ef59fa56cd0cff31`. Earlier candidate evidence and reviewed packet are `4b41ac83` and `58169f06`.

The epaint pipeline shares shaping, positioned rows, logical bounds and GUI rounding between exact measurement and full layout. Full layout retains the original input `Arc<LayoutJob>` and complete mesh/visual metadata. Measurement uses the existing normalized full-cache key and generation bookkeeping; ordinary misses retain no entry or parallel cache. Paragraph-composition requests fall back to full layout.

Released egui 0.36.2 shares WidgetText preparation between full layout and measurement, preserving overrides, fallback, color, line spacing, wrapping, alignment and existing-Galley pass-through. Only `RootListGeometryCache::ensure` requests measurement. Every absolute row remains measured. Visible widgets, the eager reference, Grid and Notes retain their full-layout paths.

Live `font.rs` remains unchanged at SHA-256 `8eae76fc33f858e2ac977ed3571d0e7de4024f3e1a1b0a590999dfac96fd5517`; all shaping/cache policies remain unchanged. No archive diagnostic timers, observer helpers, flags or Trace machinery entered production.

## Focused verification and provenance

All eleven requested tests passed against live source, locked/offline/fast-dev, with isolated targets:

| Scope | Actual Nextest run | Result |
| --- | --- | --- |
| Epaint: exact size bits, frozen stock full output, Arc identity, cache miss/generation and Unicode/cursor contracts | `ad3d5b02-e6c8-43e3-b6ae-000dbcfd5708` | 8 passed, 51 skipped |
| WidgetText formatting/fallback/spacing and Galley pass-through | `74e88c2e-bd68-4f5c-a25c-73966bd2e463` | 2 passed, 50 skipped |
| ROOT geometry against unchanged eager full layout | `9608c7b4-6c7f-4327-ab92-084083ee1735` | 1 passed, 5,235 skipped |

Direct stdout/stderr, native exit files and exact command/source metadata are under `target/egui-modernization/m2a/size-only-live-port-001/receipts/`. Passing stderr hashes are respectively `8b01d96079ec2f586ad2fb42c634cdf2d72dcce6a265767e9f6e31e4567f2cd5`, `502e8b43dd52ee3936204dc5c0a2190d513617a3bfca9f24ba64d68cd4c0ab8a` and `efb85e6d45c60108d2c9955d64530cb1518447314a97ca6493e1ba0a9746a727`; stdout was empty. Parent independently matched every test-start source binding to final live bytes and all six raw log hashes. No post-test source drift.

The first epaint attempt exited 101 for a missing Vec2 import; its original logs were preserved before the corrected passing run. The ROOT build took 5m55s. File-scoped rustfmt, diff hygiene, four locked/offline normal/feature inverse graph queries and provenance/instrumentation scans passed. These final scoped-check outputs are transcript-derived only, not separately retained raw logs; no repeat repaired that logging limit.

The private epaint harness points at live `vendor/epaint/src/lib.rs`, with a separate test-only lock and existing runtime pins/features. Production workspace membership and dev dependencies were not expanded. The two independent frozen stock fixtures retain hashes `d75c7b2cc19ffd995fd5cef7e812a75dc3d9b3bee9adc2b8c85fac4a6fadd51d` and `3ab2a7ac0b9b1bffe09f45dbfed23f9c59e6d96323708ee8981eac3c35f5e2b7`.

Vendored egui originates from released crate SHA `dc938cc27cd911415e1e4d151bc56ff9df063d8db081374e87aeb805ea74b2ba`, upstream `49682f8baa058bf49e011035cfbd6e825f88a5ef`. Its package files differ only in widget_text.rs; registry .cargo-ok is omitted, with same-upstream MIT/Apache licenses and PATCH.md added. Root manifest adds only the egui path patch; root lock removes only its registry source/checksum. One local egui/epaint 0.36.2 family and existing features remain. A PowerShell ESC typo in PATCH.md was corrected before acceptance. Independent source review found no substantive defect.

## Actual canonical release artifact

Frozen source/archive/target live under `target/egui-modernization/m2a/release-full-root-size-only-001/`. Git archive `artifacts/source-79653eca.tar`: 57,128,960 bytes, SHA `acb935e0ea33c9aefe62dec7f5407449c20c896473437038b947bd1da7b83102`. Source binding `b36ad1c80aba27b5d772ca46d56ab57abf9eb4673d54538831b527f411940183` covers 21 focused files, tools, preserved original and cached default-font crate/four assets. Parent verified archive/extracted/Git blob identities with CRLF-only normalization and binary assets exactly.

Exactly one stable Cargo/Rust 1.99 MSVC invocation used `cargo test --locked --offline --profile release --lib --no-run --message-format=json` with the frozen manifest and fresh isolated target. Native exit 0, 735.5999518 seconds. All 523 Cargo JSON messages parse; exactly one newly compiled library-test artifact, opt-level3/debug0/assertions false/overflow checks false/test true/features empty, final build-finished success. QA, profile, RUSTFLAGS and experiment overrides were absent in the exact child environment.

Built and preserved `receipts/current-release-multi_launcher-libtest.exe` match: 86,798,336 bytes, SHA `f26dcde29ceaf612ece267725b9e70e8752e583e56115df40ddb89e4c0187ec6`. Result receipt SHA `7587114fec6cf59609b132e8305501170e813dd3d38a01dce73caedb9ee30e45`; copy binding `79d9520d7430bc86b6100e52c9c1d4436fd2a14ece7802fecf229a0850f4c59a`. An initial copy helper stopped before copying on a typed expected-hash typo; the corrected binding verified without build/list/body repetition.

Both exact ignored Full-owner listings exited 0, found one owner and ran no bodies. Original/current listing receipts hash to `4e6eb2ab84d337e42c3d2d013d39c07cf20934cf2c3596fa3e7dfd1875d402a4` / `66589b24f205fe6541c2bc733cb8b90beb232aa721c604a7eb93af4d39d33701`. Candidate listing was reused for the byte-identical preserved copy. Completed artifact review accepted without findings before timing.

## Actual Full ROOT comparison — FAIL

Exactly one original f45e377/990081 invocation then one actual production79653eca/F26D invocation completed, native 0, wall28.903s/38.175s. Both correctness assertions passed. The unchanged exact owner is `gui::render::tests::track_c_benchmark_root_list_frame_geometry`, ignored, single test thread, no capture, canonical release/Full. Actual arm source commits were supplied through `ML_TRACK_C_SOURCE_SHA`; PERF and clipboard controls match the accepted recipe, with QA/overrides absent.

All twelve records and 240 elapsed values are retained: six cases per arm, five warmups/20 samples, zero exclusions, seven aligned20-value vectors. Parent and independent reviewer verified raw fields, quantiles, work counters, native/log/environment/runner/executable bindings and within-version identity stability.

| Case | Original p50 / p95 ms | Current p50 / p95 ms | Original range ms | Current range ms |
| --- | ---: | ---: | ---: | ---: |
| 100 cold | 1.3181 / 1.6105 | 1.2828 / 1.7002 | 1.1506–2.1009 | 1.1970–1.8628 |
| 100 warm | 0.1567 / 0.2203 | 0.2694 / 0.3546 | 0.1018–0.2611 | 0.2606–0.3546 |
| 1,000 cold | 11.2742 / 16.5789 | 15.8411 / 17.7901 | 10.5613–16.7666 | 15.4875–27.1029 |
| 1,000 warm | 0.1832 / 0.2370 | 0.2847 / 0.3849 | 0.1590–0.2748 | 0.2611–0.6334 |
| 10,000 cold | 127.5695 / 149.0629 | 166.2805 / 179.1976 | 118.1821–179.8763 | 158.4539–182.6321 |
| 10,000 warm | 0.2003 / 0.2771 | 0.3171 / 0.3891 | 0.1916–0.3553 | 0.3048–0.4530 |

Current medians are slower in five of six cases. At10k cold the descriptive median delta is +30.345%; warm +58.313%. Full10k cold ranges overlap; current minimum158.4539ms exceeds original p95149.0629ms. Do not claim disjoint ranges or trim the current1k cold27.1029ms sample. The100cold median is comparable/slightly lower while its p95 is higher.

Limits remain explicit: fixed original-first order, a nonexclusive local Get-Process compiler snapshot with CIM enumeration denied, original26/current25 painted rows, and distinct old Context.run/current logic-ui-finalizer boundaries. Cross-version output identity equality is not asserted. Historical paragraph-only/new-port differences are unpaired and cannot establish causal production savings.

Matched index: `body-runs/matched-full-root-size-only-results-001.json`, SHA `5e41fec2f480d5282ccd66b9d4511402ebec72e830690430c7df590a688c8f9b`. Original/current parsed hashes: `f642e79e8213a0397cd64498043beead235706b9926c97b67369db8c8247814c` / `aeffbb924bcc09408afcd46c58f875ad0db2339858a1cbaa5991a2d47c0833e0`. Direct runner SHA `bcf3c0733e7780e7f43d1afd182ec873c18200c9407942d1fd7d2724a73d9dfe`; raw logs, native results, preflights and all arrays are preserved beside the parsed outputs.

Independent result review accepts the measurement evidence without integrity findings and retains performance FAIL. No repeat, exclusion, automatic optimization, performance waiver or Track B entry follows. A bounded read-only planner investigation of the remaining cold-layout and warm-frame owners is in progress; no further source change or body is authorized. All remaining mission gates stay required. Checkpoints remain local; Workspaces/ is untouched.
