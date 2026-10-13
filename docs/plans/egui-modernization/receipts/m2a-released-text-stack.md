# M2-A isolated released text-stack comparison

Independent source, dependency, build, parity and timing reviews accepted this experiment without findings. Production remains at `87e08b32`; adoption requires a separate implementation packet and focused verification. M2-A remains in progress.

## Scope and compatibility

Fresh paired archives under `target/egui-modernization/m2a/released-text-stack-candidate/` preserve the accepted earlier archive. Baseline uses released HarfRust 0.12.0 / Skrifa 0.44.0; candidate uses HarfRust 0.13.3 / Skrifa 0.46.2 / read-fonts 0.43.3. Their reader types align. Existing font-types 0.12.6 and once_cell 1.21.4 remain unchanged. Other consumers retain the old Fontations versions. The only graph changes are the required text stack and dependency-reference disambiguation; features and default font bytes are preserved.

Both copies remove the earlier residual HarfRust timing bridge, feature activation and path patch. They retain identical epaint observers, retained-plan behavior, preparation and owner timers. Candidate compilation required no API source adjustments. Exact downloaded package checksums match their lock entries; metadata downloaded the three new packages, so an earlier statement that they were not fetched was corrected. No profile, renderer, production source or fixture changes.

## Verification and full parity

Both locked/offline fast-dev libtest builds passed: baseline 166.712 s, candidate 177.746 s. Snapshot-only Nextest runs passed in 4.730 / 4.672 s. Each complete canonical snapshot is 4,964,903 bytes, SHA256 `66D25A29A4A62B08A8E12E83CB7204012FEA7C65F2F1528DBC756563E740712C`.

Independent review byte-compared the complete files, including 4,047 ordered run records, 103,830 glyph records, all glyph properties, exact floating-point bits for cached/eager geometry, global extents, scroll/viewport data and painted action IDs/rects. Only process-local face keys are omitted. The fixed default-font fixture passes exact cross-version output parity; this is not general variable-font compatibility evidence. Each timing process freshly regenerates its snapshot and checks both stored versions before warmups or measurement.

An initial direct cargo-nextest shim invocation failed before test launch, exit 2; its logs remain preserved. Correct `cargo nextest run` invocations succeeded. No failed result was relabeled.

## Bounded timing

One thousand cold rows, existing nine-pass preparation outside timing, two warmups and five fresh-context samples per version. Both exact ignored Nextest runs passed, native exit 0, wall 16.238 / 16.234 s. Parent independently parsed all ten raw samples and recomputed medians; reviewer verified counters and artifact bindings.

| Metric | Baseline | Candidate |
|---|---:|---:|
| Owner p50, ms | 390.7041 | 370.1820 |
| Retained-plan shaping p50, ms | 210.4507 | 192.3378 |
| Owner samples, ms | 388.7256, 393.4444, 390.7041, 409.1165, 389.8630 | 370.8452, 370.1820, 373.5813, 368.3457, 368.5185 |
| Shaping samples, ms | 209.7815, 212.4397, 210.4507, 218.0461, 209.6304 | 194.0592, 192.2982, 194.6301, 191.8751, 192.3378 |

All samples retain 4,047 plan hits/shape calls, zero misses/builds/evictions, 103,830 glyphs, one rebuild, 1,000 measured rows, 25 painted rows and one adapter pass. Preparation remains 4,150 lookups / 4,147 hits / three builds. Output identity `bd4860c5d2427fa2`, structural signature `18c853161fe60dfc` and viewport receipt `52001c291b514345` are unchanged.

Descriptive median reductions are 5.2526% owner and 8.6067% shaping. Every candidate owner sample is below every baseline sample in this limited series. Sequential baseline-then-candidate order and five diagnostic samples limit causal/general claims. This supports considering a partial adoption; it establishes neither original-version parity nor warm, release or native benefit. It does not prove that upstream issue 256 is fixed. No Full81 run or M2-A acceptance.

## Evidence identities

All raw evidence remains in the paired archive's `receipts/` directory. Source/graph freeze receipt SHA256 `5290B14686537B9E2A8671334221B6EBF20071A5D38980F05C5BA27F9AACA557`; parity receipt `EA8D47ADD09F3BC5908D780F9C46614F46FB8CD1233BBAA4707B20DA4F919740`; timing receipt `69B2513E1640B63B43C99B8D680D20D8D79BED0919A73D352B544F7A274623EB`.

Preserved baseline binary: 138,933,248 bytes, SHA256 `81C81EF5FAE9DA6B2BB44259203537FCA5AF1D526245A143E303F3C8695FD5A2`. Candidate: 139,279,360 bytes, SHA256 `DE54B171F06292A7141989AE0EBC66C20E079D16BAC74060E00369A388C06522`. Cargo JSON selects the actual artifacts. Raw timing log hashes: baseline `E0C2003F923A6C40F5C460E2270C301D2EE36C71198A733B49517B8539927741`; candidate `3FB800A4235BDE11336303506410951F6116979AFA61C8E0B1F45F1D49048E85`.

## Separate repeated-input analysis

Read-only analysis of the baseline canonical trace found 1,001 unique input keys: 975 singletons and 26 repeated keys. Of 4,047 calls, 3,046 repeat an earlier identical input (75.27%), accounting for 76,260 glyphs (73.45%). Complete output arrays match for every repeated key. Parent independently reproduced these counts. These are duplicate-work counts, not CPU savings or cache authorization.

Keys include every recorded run-header input except ordinal and output glyph count. Source confirms this fixture uses collection index zero, empty variations, no actual instance and fixed default shaping options. The trace omits general normalized-instance, collection-index and option/policy identities; it must not be treated as a general cache key. See the correctness boundary in implementation-handoffs.md. No shaping-result cache was implemented.
