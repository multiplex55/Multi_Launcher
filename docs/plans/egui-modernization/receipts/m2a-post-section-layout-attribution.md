# M2-A post-section layout attribution archive

The bounded experiment is complete and independently accepted with no remaining findings. Production remains at `9b7006cd`; this diagnostic does not establish M2-A performance parity.

## Question and scope

Measure work after shaping and glyph emission: intrinsic sizing, wrapping, positioning and CPU text-mesh construction. Four aggregate clocks cover an inclusive post-section interval and its three disjoint calls to `calculate_intrinsic_size`, `rows_from_paragraphs` and `galley_from_rows`. Checked subtraction reports the remaining inclusive work. No per-row/per-glyph clocks or additional cache.

One ignored archive at `target/egui-modernization/m2a/post-shape-layout-attribution/root` derives from tracked production checkpoint `9b7006cd47e052ab1d608cc3526a808d36898536`. Only six archived observer/owner files may change. Raw-result caching and FIFO8 plans remain unconditional, including normal nine-pass preparation. Existing observers are identical across controls; off/on varies only the new clocks.

## Baseline integrity

Preserved Git inventory: 1,069 entries, SHA256 `28D93E27A28FF53D1E212F4D63E3EE111A44547AAA9F31236BDADA6F45ADD1D2`. Preserved ZIP SHA256 `3EC3B42061709C4F71C32AF8ABB3FB63E70F8A9C796D0130BB2C65A155FCDE6A`.

Parent independently checked the ZIP against every Git blob hash and length: 28 raw-byte matches and 1,041 matches after CRLF-to-LF normalization, no other differences. The initial raw-only assertion failed on a text file; the corrected audit identifies Windows line endings, not a source repair. A subsequent hook-scope audit found 1,064 normalized unchanged tracked files and changes only in the five assigned existing files. The new diagnostic module is the sixth assigned file.

Accepted full canonical reference: 4,964,903 bytes, SHA256 `66D25A29A4A62B08A8E12E83CB7204012FEA7C65F2F1528DBC756563E740712C`; parent verified the archive's copied reference.

## Construction and review

The writer added observer hooks and `track_c_diagnostic_root_list_post_shape_layout_attribution`. Default parity-only mode, fresh output directories, complete canonical comparison, immediate observation freeze and explicit later timing authorization are retained. Timing must regenerate full parity and bind every warmup/sample to accepted identities and work counts.

Provisional review identified and closed three corrections:

- Capture each new child and parent elapsed time before counter bookkeeping.
- Report maximum per-face shallow metadata as actual entry count multiplied by entry size.
- Adapt three preserved private test callers to the observer capture argument and returned carrier, retaining unknown provenance for existing plan assertions.

The diagnostic also requires an actual viewport receipt rather than inventing a fallback identity. Final reviewed hashes: font `E6653C1375294BF68F8B7F7C7DF4172CEE54B04BC96189A920B7A244E149EA62`; layout `40E96521C7C589AF09B4274E8D575C3CC4B7BE2EAC0AED0C509B5839848EA16D`; root `C4FF230A293994D9D9ACB6DE3398DA6457498F834B3D0C900C7D4BBB278CBB82`.

## Compilation evidence

The first output-persistence attempt is unavailable and is not counted as success. A captured attempt failed with native 101 opening the shared target lock; command termination and absence of Cargo/rustc/Nextest processes were checked before a scoped permission retry. No lock deletion, ACL or target change.

The scoped retry reached compilation and failed, native 101 / 95.068 s, on two optional viewport-receipt formatting sites; `artifact-receipt-004.txt` retains the output. After that correction, compilation-only receipt006 passed, native 0 / 160.86 s. After the three review corrections, final compilation-only receipt008 passed, native 0 / 60.7876973 s. These compilation commands executed no test body.

Nextest binaries-only metadata returned native 0 / 1.8370873 s; direct `--list` confirmed the named owner without executing it. Generated and preserved executable SHA256 is `1739C75DE085E86FD82511AB2E607EAEFA7B9DBEFD531A500055BB00300C4865` (139,167,232 bytes).

Parent verified the archive lockfile is byte-identical to the ZIP baseline (191,924 bytes): raw SHA256 `E9B32A0EE710FF4728AC31B2A6B0B67F9B2E384E4AE3622798E4DCCBA247035F`, LF-normalized SHA256 `0AA62F31D471E9B42F59209361EBC756572B5EFF682351668A03F212FD1EDA8C`. Its difference from the live lock hash is line endings, not dependency resolution.

Final binding bundle `source-binding-final013.json`, SHA256 `F3ACB25E14A2ECCDF923C3B4A7E48AC454D9197DE848E43FB026ED633E2CC32D`, records expanded executable argument vectors, environment, source, lock, reference and preserved executable identities. The inherited PATH suffix and direct-list wall time were not retained and are explicitly unavailable. Parent independently verified the bundle hash, six current source hashes, six listed receipts, baseline ZIP, reference and both generated/preserved binary hashes. Writer stopped at this gate without running a body.

Independent final binding review accepted the gate with no findings. Parent then authorized parity-only execution on frozen inputs, with explicit source attribution and a fresh empty output directory; logs remained outside that directory. Actual parity review preceded timing authorization, as documented below. No production optimization, full suite, release/native qualification or performance waiver was authorized by this packet.

## Actual parity

Initial run001 reported a test pass and wrote snapshots, but its post-test PowerShell wrapper failed while serializing a bare `false`. Native Cargo exit and wall time are unavailable, not inferred. The first logs, snapshots and wrapper error remain preserved. Parent authorized one corrected parity-only retry with unchanged source/binary and a new empty directory.

Run002 passed one ignored owner, native exit 0 / 4.1802613 s. Source and binary hashes match before/after. Complete off/on snapshots are each 4,964,903 bytes, SHA256 `66D25A29A4A62B08A8E12E83CB7204012FEA7C65F2F1528DBC756563E740712C`; parent byte-compared both complete files against the accepted reference. They retain 1,000 actions, 4,047 ordered runs, 103,830 glyphs, 25 painted rows, work counters `[1, 1000, 25]`, revisions `[2, 1, 1000]` and identity tuple `bd4860c5d2427fa2 / 18c853161fe60dfc / 52001c291b514345`.

Review index `parity-review-index-003.json`, SHA256 `A08F972400182EA99CA1E9C79AEB2336686587280B6CEC633BE04D7E3BB7F9F8`, binds the attempts and artifacts. Successful test stderr was suppressed because the command omitted `--success-output final`; raw post-section call totals are unavailable in those logs. Executed assertions checked clocks-off zero calls, clocks-on positive/equal child calls and logical work consistency. No warmup or sample series ran during parity. Independent review accepted actual parity without another run; no unrecorded call total is inferred.

Parent then authorized exactly one unchanged-binary timing invocation in a fresh empty directory, with `--success-output final`, regenerated full canonical parity, two warmups and five samples per clock mode. Its accepted results follow.

## Controlled timing

Timing-run001 passed, native exit 0 / 16.8929477 s command wall. The unchanged binary regenerated complete parity first, then retained four warmups and ten samples, none excluded. Successful output is retained. Parent independently parsed every sample and warmup identity, checked nested arithmetic and logical work, and recomputed medians.

| New-clock mode | Owner samples, ms | Owner p50, ms |
|---|---|---:|
| Off | 222.6206, 223.8889, 226.0966, 228.4609, 226.4899 | 226.0966 |
| On | 303.3816, 245.9854, 226.7062, 239.8295, 223.8785 | 239.8295 |

| New interval, clocks on | Samples, ms | p50, ms |
|---|---|---:|
| Post-section inclusive | 45.2682, 36.8283, 34.5939, 35.9742, 33.1165 | 35.9742 |
| Intrinsic size | 2.6096, 2.3129, 2.2791, 2.3351, 2.2345 | 2.3129 |
| Rows from paragraphs | 8.2082, 6.6806, 6.1835, 6.3209, 5.5876 | 6.3209 |
| Galley finalization, including positioning and tessellation | 33.7068, 27.2405, 25.5766, 26.7317, 24.7476 | 26.7317 |
| Checked remaining inclusive work | 0.7436, 0.5943, 0.5547, 0.5865, 0.5468 | 0.5865 |

Each on sample has 975 calls for the inclusive parent and each child; off has zero new-clock calls/times. These are actual layout misses, not an assumed 1,000 calls. Every record balances parent minus its three children exactly; independently selected medians need not sum. The inclusive galley-cache miss timer is another parent, not an additional cost.

All ten samples have 55 galley hits / 975 misses, 4,047 face runs, 103,830 emitted glyphs and glyph-cache hits, zero glyph allocation misses, 3,072 raw-result hits / 975 misses / zero bypasses. Each retains one rebuild, 1,000 measured rows, 25 painted rows, one pass, no geometry errors/abandonment and the accepted identity tuple. Raw maxima are 128 entries / 175,833 payload bytes / 14,336 shallow metadata bytes per face. Normal preparation explains why these hit/miss counts differ from the earlier empty-cache prototype.

Existing observations in the off control have p50 section 177.2914 ms, segmentation 27.4047 ms, native shaping 46.7807 ms and shaped-run emission 61.3381 ms. These nested observations are not additive independent owners. Post-section work is measurable but does not dominate the current owner; it is not a measured old-versus-new regression allocation.

The on owner median is about 6.1% higher. Its first sample is high across existing stages as well as the new intervals, and its last sample overlaps the off range. Five sequential samples per mode do not isolate all of that difference as clock overhead. Both controls retain the existing observer envelope, so off is not uninstrumented production. No clean overhead estimate, speedup, original-version parity or release/native conclusion is claimed.

Timing review index `timing-review-index-002.json`, SHA256 `DE863083B2BE4CDCAA214CE6EA8D226B567999B540844EAE45C3CA062E7E80A7`, binds raw commands, environment, logs, samples, regenerated snapshots and unchanged source/binary. Independent final review verified all samples, warmups, arithmetic, complete snapshots and current bindings, accepting bounded descriptive evidence with no findings. Writer released. No additional instrumentation or optimization follows automatically from this experiment.
