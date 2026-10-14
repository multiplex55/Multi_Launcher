# M2-A matched release Full ROOT control

Status: measurement complete and independently accepted as bounded evidence, with no evidence-integrity findings. **Performance parity is not achieved.** The current owner remains slower in all six cases. M2-A and Track B remain gated.

## Inputs and execution

Compared original `f45e377390284cc1ba6cf474fcd23e008514318e` with production adoption `3a10f1de6a1f6be5093d3bcf4be664e32d5b3f76`. Both use canonical release, stable Rust/Cargo1.99.0 MSVC, ordinary features, no QA request/feature and no profile/Rustflags overrides. The current source is a fresh Git archive with an isolated target. Parent independently verified all1,072 files against its inventory and Git blobs:28 byte-exact and1,044 exact after CRLF presentation normalization, with no other differences. Independent input review accepted manifests, locks, relevant source rows, toolchain and font bindings.

The original's previously accepted executable was reused unchanged. Its missing Nextest archive/metadata prevents immutable Nextest reuse; both versions therefore use the same clearly labeled direct libtest runner. No metadata was fabricated and the original was not rebuilt.

Current build-only invocation was locked/offline `cargo test --profile release --lib --no-run --message-format=json`, with explicit frozen manifest, cwd and fresh target. It passed with native0/713.6119133seconds. Actual Cargo JSON selected exactly one fresh workspace library-test executable, profile opt-level3/debug0/debug-assertions false/overflow checks false, followed by successful build-finished. Parent and reviewer checked this selection; no filename was guessed. Both exact ignored-owner listings passed without test bodies.

| Artifact | SHA256 |
| --- | --- |
| Original preserved executable,80,087,552bytes | `990081b657ce1a270690bf29c9178f576f8c75e8a9289afea6f8aa2937de4f6d` |
| Current preserved executable,86,876,672bytes | `2339e81d937e812285b16ff0f3af7e200d7d6c5c5725bc58891a85a8697123a9` |
| Pre-body gate001 | `fa7db37a1c721eb5e74fb3bfc752f7a235c6251003d2f0934bc64b3b1b0c6c52` |
| Current input binding001 | `151ed4dec5f558baec9ad3c43b626b64ed5d512dd0b1374cdc3b7536e925d724` |
| Results001 | `090ad1b0cbfe83bd0cc7bbdb9a22921c4e7266032f9f192759780d6a028bbeba` |

After independent artifact clearance and separate parent authorization, each executable ran exactly once, original then current:

```text
<bound-executable> --exact gui::render::tests::track_c_benchmark_root_list_frame_geometry --ignored --test-threads 1 --no-capture
```

Environment: `MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`, `ML_TRACK_C_BENCH_MODE=full`, `ML_TRACK_C_PROFILE=release`, and each actual committed source SHA. Each used its own frozen cwd, fresh child environment/evidence directory and the owner's isolated TrackCWorkspace. The owned build was terminal and named Cargo/rustc process observation was empty before execution. An unavailable CIM query is recorded as such; it is not a claim about all unrelated host activity.

Both exact-owner bodies passed1/1 with native exit0. Original process wall was29.042664seconds; current46.553829seconds. These envelopes include preparation/validation and are not owner latency or application-launch measurements. Sources and executable hashes remained unchanged.

## Retained owner results

All12 existing records retain five warmups and20 measured samples per case:240 measurements total, zero exclusions and no benchmark reruns. Arrays remain in measurement order and aligned with output/structural IDs, revision/viewport receipts and work fields. Quantiles use separate sorted copies:10th/19th/20th values for p50/p95/max. Parent and reviewer independently parsed the raw logs and recomputed these values. Review verified all seven aligned vectors, fixture joins, work counts, invocation/log hashes and unchanged source/executable bindings without evidence-integrity findings.

| Rows / geometry | Original p50 / p95 ms | Current p50 / p95 ms | p50 delta ms | p50 ratio |
| --- | ---: | ---: | ---: | ---: |
| 100 cold | 1.3030 / 1.8443 | 1.9867 / 3.0974 | +0.6837 | 1.525 |
| 100 warm | 0.1636 / 0.2079 | 0.2740 / 0.3834 | +0.1104 | 1.675 |
| 1,000 cold | 11.5831 / 18.1951 | 21.0992 / 29.6441 | +9.5161 | 1.822 |
| 1,000 warm | 0.1745 / 0.2540 | 0.2989 / 0.4576 | +0.1244 | 1.713 |
| 10,000 cold | 126.1726 / 142.3534 | 235.4586 / 248.7747 | +109.2860 | 1.866 |
| 10,000 warm | 0.2057 / 0.2589 | 0.3284 / 0.4473 | +0.1227 | 1.596 |

For10,000 cold rows, original retained values range118.3374–143.8934ms and current226.0037–255.9763ms: ranges do not overlap. The result demonstrates a remaining meaningful cold-owner regression; no arbitrary percentage allowance or performance waiver is applied. One sequential matched run per version does not characterize all host-to-host or run-to-run variation.

Common fixture signatures/counts, viewport960x640, scale1000, settings and cold/warm semantics match. Cold records have one rebuild andN measured rows; warm records have zero rebuilds/measured rows. Original paints26 rows and current25; that difference is preserved. Output/structural identities are stable within each version/case and are not forced equal across versions. Original owner label is `egui_Context_run+LauncherApp::render_root_frame`; current label includes `run_ui`, root logic, rendering and receipt completion. Both retain their actual labels.

Each version retains its real font assets. Ubuntu-Light, Hack-Regular and NotoEmoji match; emoji-icon-font differs and is disclosed in the eight asset bindings. Current locked text stack is HarfRust0.13.3/Skrifa0.46.2/read-fonts0.43.3; older optional/other-path lock entries were not silently upgraded.

## Evidence and limits

Ignored evidence root: `target/egui-modernization/m2a/release-full-root-control-001/`. Raw runs are `runs/{original,current}-full-001/`; full ordered arrays, min/max/spread and binding checks are in `runs/matched-release-full-results-001.json`.

The initial six-entry inventory was caught before compilation and superseded by the complete inventory; expected CRLF-versus-Git-blob differences were recorded without source repair. Two result-index builder failures were analysis-only (PowerShell nested-array flattening and the pipe delimiter for work counters); no result index was written by those failed attempts, and no benchmark was rerun. The corrected Python parser uses field-specific delimiters and retains every value.

This accepts no performance gate. It covers the ROOT CPU owner only, not grid/notes/query, native windows, renderer/GPU, launch/idle or Full81. Source remains at production3a10f1de. Further root-cause work must address the remaining regression before theming; no additional optimization or measurement follows automatically from this receipt.
