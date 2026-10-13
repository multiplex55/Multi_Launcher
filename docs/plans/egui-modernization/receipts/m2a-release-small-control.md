# M2-A matched release Small control

Canonical release, serial original `f45e377390284cc1ba6cf474fcd23e008514318e` then current `6606f5e9c3122eea0093c03ece354fa1b8ac4bcc`, in fresh ignored Git archives. Neither manifest defines a release override; iteration was not used. Own manifests, locks and vendor patches retained. Archive EOL conversion is verified against Git blobs. No live source or profile changes.

Command in each archive: `cargo nextest run --locked --offline --cargo-profile release --profile default --lib --run-ignored ignored-only -E 'test(track_c_benchmark_root_list_frame_geometry)' --success-output final`. Perf/skipclipboard enabled, Small mode, actual source/profile attribution; QA feature and request absent. Fixture `root-list-default-layout`, signature `c3745052c65797f2`,100 actions,960x640,scale1000,five warmups/twenty samples,zero exclusions.

| Case | Original p50 / p95 ms | Current p50 / p95 ms | p50 / p95 ratio |
|---|---:|---:|---:|
| Cold geometry, warm fonts | 1.2855 / 1.4116 | 4.6539 / 6.8441 | 3.620 / 4.848 |
| Warm geometry, warm fonts | 0.1601 / 0.1929 | 0.2692 / 0.3819 | 1.681 / 1.980 |

Both confirmed native exits0 and owner1/1 PASS. Original direct rerun002 wall4.853s, cached build0.94s, owner2.72s; current001 wall580.042s, build9m35s, owner2.73s. Original first build8m17s/test PASS retained, but its Start-Process wrapper hung after native processes disappeared; interrupted wrapper has no confirmed exit envelope. Direct rerun002 supplies the original comparison without source change. These unit-test build times are not application-build comparisons.

Parent independently parsed all four records, recomputed nearest-rank p50/p95/max, verified20 retained samples, fixture fields and work counters. Cold counters1 rebuild/100 measured rows; warm0/0. Original paints26 rows/current25, disclosed rather than hidden. Independent reviewer accepted source/profile/provenance/raw quantiles/hashes with no findings. This confirms a release CPU-owner regression; telemetry-enabled libtests do not establish native/display/GPU latency. Small supplements Full81 and does not waive the fast-dev failure. M2-A remains in progress.

Local raw evidence: `target/egui-modernization/m2a/release-small-control/receipts/`, including source-toolchain-provenance-002.txt, archive-provenance-002.txt, matched-release-small-summary-001.txt, command/run/binary receipts and all logs/arrays. Original002 binary SHA256 `990081b657ce1a270690bf29c9178f576f8c75e8a9289afea6f8aa2937de4f6d`; current001 `eafd7d59e56650203a96e45aa6e4f19c9bd84eaef47fdf1e47bc9cbb3074bc21`. Original002 stderr SHA256 `ea278d0542c77efbe72b93df88be52c69d37684bcfc4ddedf3e096df2f9c33ef`; current001 `ae09be352f4dc704c51ec1a0c46debfd70d20ffda51ad43664035674613ec0c7`.

Next: execute the persisted bounded per-FontFace ShapePlan reuse prototype in the existing ignored actual-layout archive. No production vendoring until measured actual savings, bounded ownership and exact parity are reviewed. Warm UI cost remains a separate unresolved contributor.
