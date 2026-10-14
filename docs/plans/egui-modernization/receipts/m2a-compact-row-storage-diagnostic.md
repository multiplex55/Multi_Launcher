# M2-A private row-storage diagnostic

Plan checkpoint: `4d65a733`. Reviewer accepted the plan, requiring outer-vector byte counts to exclude nested glyph/Arc/mesh storage and allocator overhead. Pointer equality is an observation; fast-dev evidence does not establish release allocation or latency behavior.

Fresh ignored candidate: `target/egui-modernization/m2a/compact-storage-diagnostic-001/root`, copied from frozen compact C2E5. Parent compared all 1,207 files: no additions/removals; only `vendor/epaint/src/text/text_layout.rs` differs. The change extends the existing cold-atlas contract with an independent fresh-Fonts conversion of the same job through actual private full-layout helpers. Original assertions remain; exact serialized full output is compared. No production code, timing, allocator tracker or new fixture/test was added. Frozen compact source/artifact and live production source remain unchanged.

Source SHA-256 `bbb42caf2ccedd3b58220975faaad96e5a2c23225fdda2652edf5d1896b7c569`; patch `df43508e0bf3e1de98f3bdd730403ed0937e341d940888c77c955b4cf146168a`; provenance `2ce9e78a68694ab4278dbbb3c24b0515df23be3de50c4e3abc64506070fd9bc4`. Copied harness manifest E3DEAEF4 and lock FC8FA305 bind this candidate; canonical fixtures remain D75C/3AB2. Scoped rustfmt exited 0.

Parent ran exactly one filter, `text::text_layout::tests::compact_measurement_preserves_cold_atlas_delta_and_size`, with locked/offline Nextest fast-dev lib harness and the released compact private target. Native exit 0; wall 6.2104641s; Nextest `eb1cce9c-550a-4cb4-8009-cfcd6c1e2303`, one passed/59 skipped. Evidence: `compact-storage-diagnostic-001/receipts/private-contract-001`. Input `6679709d7cd0b8a094e982ab2217d9e25f7ff5221314b59db5588c0cd9bc41cb`; stdout `ebfbd9c1d2b8b250907de09a307568d698345a0b331c76eb497f54d84f3530bc`; stderr `e806f5366db830ada6f4998fe25e203f8b45074da167955a3f202aeb48dd2eec`. Source, harness, lock and fixtures stayed bound. Four inherited fonts.rs deprecation warnings remain outside this test-only scope.

| Actual outer-vector observation | Source | Destination |
|---|---|---|
| Element type | LayoutRow<Glyph> | PlacedRow |
| Element size / alignment | 48 / 8 bytes | 24 / 8 bytes |
| Length / capacity | 2 / 4 | 2 / 8 |
| Capacity × element size | 192 bytes | 192 bytes |

Allocation pointer equality was true; no addresses were emitted. Full serialization, cold atlas/image/delta and exact size checks passed; glyph representation sizes remain 68B/20B. This confirms retained wider outer-row backing on this fast-dev fixture. It does not establish a latency cause, release allocation behavior, nested-allocation totals, or performance qualification. Independent completed review accepted without findings: test-only diff, bindings, single native0 pass and bounded interpretation verified. Planner preparing a read-only bounded repair proposal. No repair, additional benchmark, production port or Track B clearance follows from this run.
