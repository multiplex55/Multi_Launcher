# M2-A archive raw-shaping reuse prototype

Independent source, private-contract, owner, canonical parity and timing reviews accepted this prototype without remaining findings. Production remains at `fc9edf1a`; no raw-result cache has entered production. M2-A performance acceptance remains open.

## Ownership and bounds

Fresh ignored copies under `target/egui-modernization/m2a/raw-shaping-reuse-prototype/candidate/` use the accepted HarfRust 0.13.3 / Skrifa 0.46.2 stack. `FontFace` owns an initially empty raw-result LRU beside its unchanged FIFO8 plan cache. Admission requires actual `ShaperInstance=None` and the layout owner's proven fresh/cleared buffer lifecycle. Variable instances, including face-tweak variations, and unknown/unsupported buffer policies bypass unchanged. The key covers exact text and resolved script/direction/language/flags; locally fixed shaping options remain explicit. Plan feature-variation identity is not reused as raw-result identity.

An owned carrier contains either the native `GlyphBuffer` or immutable shared complete glyph arrays plus the incoming `UnicodeBuffer`. Emission borrows its slices without retaining a face borrow. Consuming the carrier clears and recycles the original buffer. No replacement allocation on hits, raster/galley cache or new dependency.

Fixed limits: 128 LRU entries and 1 MiB retained key/glyph payload per face; 4 KiB text and 64 KiB payload per entry. Checked admission rejects oversize/overflow before disturbing existing entries. Metadata and temporary shared handles are separately disclosed; payload is not total heap/RSS. A replaced face starts empty.

Parent compared the preserved input inventory after implementation: only the four authorized runtime/owner files changed among 1,059 source inputs, plus the private harness manifest's archived-library target. Other inventoried source/assets/locks remain unchanged. Live Git reports no production source change.

## Focused correctness and execution gates

Final private filter007 passed 4/4, native 0, command 3.859 s. It covers key/face isolation, LRU and all admission limits, multilingual full-output parity, actual variable/face-tweak bypass and Bahnschrift extrema. Recycling uses a native miss → cached hit → shorter forced native miss with pointer equality. Unsupported-policy fixtures first prove an observable difference from defaults, then exact equality to identically configured cache-off references. New contract helpers compile only under tests.

Initial compiler, accounting and insufficient-fixture attempts remain preserved. Root no-run004 passed, native 0, 58.142 s; no test body executed. Cancelled003 records intentional interruption for source corrections, tool exit 1 and subsequent absence of cargo/rustc processes; native Cargo completion and PID ancestry are explicitly unavailable.

The owner defaults to a parity-only phase that returns before warmups or samples. Explicit timing regenerates the complete gate in a fresh empty output directory. Every warmup/sample must match the accepted output, structural and viewport identity tuple. Raw-result reuse is disabled during identical nine-pass preparation and starts empty immediately before the enabled owner. Freeze follows elapsed capture; validation and disposal stay outside timing.

Parity-only run002 passed, native 0, command 5.591 s, one test. Cache-off, cache-on and accepted reference files are each 4,964,903 bytes with SHA256 `66D25A29A4A62B08A8E12E83CB7204012FEA7C65F2F1528DBC756563E740712C`. Independent review byte-compared all complete files: 4,047 runs, 103,830 glyphs, exact cached/eager geometry, scroll/viewport and painted IDs. The log records zero timing owners and no warmup/sample records. Initial run001 failed the empty-directory guard because its receipt was inside the output directory; it produced no snapshots and is preserved.

## Approved timing result

One unchanged binary, 1,000 cold rows, two warmups and five fresh-context samples per mode. Native exit 0; command 28.311 s / test 26.224 s. Full canonical parity regenerated before timing. Reviewer and parent independently parsed all ten raw sample records; none excluded.

| Metric | Reuse off | Reuse on |
|---|---:|---:|
| Owner p50, ms | 373.0338 | 227.9875 |
| Recorded native shaping p50, ms | 191.3224 | 49.3760 |
| Owner samples, ms | 373.0363, 373.0338, 371.6989, 379.6210, 372.9876 | 225.2094, 242.1825, 227.0502, 236.6994, 227.9875 |
| Native shaping calls | 4,047 | 1,001 |
| Raw hits / misses | disabled | 3,046 / 1,001 |

Every sample retains 4,047 plan lookups/hits, zero plan misses/builds, 103,830 emitted glyphs, one rebuild, 1,000 measured rows, 25 painted rows and one pass. Preparation remains 4,150 lookups / 4,147 hits / three builds. Output `bd4860c5d2427fa2`, structural `18c853161fe60dfc` and viewport `52001c291b514345` identities match accepted parity.

Each enabled sample records 1,001 insertions and 872 evictions, zero bypasses, maximum per-face 128 entries / 175,833 payload bytes / 14,336 shallow entry-metadata bytes. The 1,132,348 inserted payload bytes are cumulative copy work across faces/evictions, not retained memory. Maxima exclude vector spare capacity, shared-pointer headers, allocator overhead and temporary evicted-handle retention. Actual owner cost includes lookup, copying and replacement work.

Descriptive owner reduction is 38.8829%. Five sequential off-then-on samples in a fixed default-font fast-dev fixture do not establish general, warm, release or native benefit. The substantial original-version gap remains. This supports a separately scoped partial production integration proposal; no automatic adoption, Full81 pass or M2-A waiver.

## Evidence identities

Full raw evidence is in the prototype's `receipts/` directory. Final ten-file source manifest SHA256 `4F4CE0C6092301D159EDD33C0ADAFF436E563B012D30062EAA22F6F1C32CDA44`; root lock `0AA62F31D471E9B42F59209361EBC756572B5EFF682351668A03F212FD1EDA8C`. Nextest JSON selects the preserved libtest binary SHA256 `043767BF3BACDA21ED2625E701A04B195E0FC04940292F330978A645D593B377`.

Parity handoff SHA256 `C52BDF535069D5C11FC97F4B80FA53E025899E9E67DCF69ED5EFBE688AE4915F`; timing handoff `CA771C067FD1E533B77705C1E6B513380E067904C56B23A28DD0FE87B69EBD11`; raw timing stderr `D1416910FFF47BFD6A5901A74DB7CDC2B7C4C3C5D27251CBF22DB99575B973EB`; execution receipt `24809A208934CFB03801840D68AC4589C530BA244B66B1876010F2B069A8C428`. Source, binary and logs remained unchanged through the runs.
