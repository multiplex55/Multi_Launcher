# M2-A glyph-allocation prehash experiment

Status: experiment completed and independently reviewed without evidence findings. Candidate rejected for production because the matched pair demonstrates no benefit. Production remains `3a10f1de`; M2-A performance remains FAIL/open.

## Candidate and ownership

Accepted release attribution checkpoint `cd2fe206` records103,830 glyph-allocation cache hits per measured frame, with shaped-run emission the largest measured text-section substage. Those instrumented timings are not a claim about removable production cost.

Source inspection found a narrower avoidable operation: `GlyphCacheKey::new` produces a prehashed `u64`, but `FontFace::glyph_alloc_cache` uses an ahash map that hashes it again. `GlyphCacheKey` already implements `nohash_hasher::IsEnabled`, and its derived `Hash` writes one `u64`. The experiment uses the existing dependency's `IntMap`, preserving the key's generation, equality, inputs and allocation behavior. Benefit is unknown; hash-table probe distribution may differ.

The fresh ignored candidate is `target/egui-modernization/m2a/glyph-allocation-prehash-experiment/root/`, copied from accepted `release-profile-adapter-001/root/`. Its only runtime change is the concrete map type in `vendor/epaint/src/text/font.rs`; initialization remains unchanged. One private allocation test is added in the same file. Existing observer code, keys, cache limits, shaping, hinting, face selection, raw LRU and FIFO8 plans remain unchanged. Baseline source/archive/reference and production files are preserved.

## Private validation

The copied private harness retains the accepted dependency pins, `default_fonts` and identical lock; only `[lib].path` points to the new candidate. Ten targeted Nextest contracts passed with native exit0 in6.2568036seconds,45 tests filtered out. No broader suite ran.

The nine existing contracts cover face-scoped plan keys/FIFO, multilingual and actual variable-font glyph parity, raw-cache keys/LRU/bounds/bypasses/recycling and early raw-hit return. The new test exercises real Bahnschrift allocations: one numeric hash write; same-bin UV/entry reuse without atlas allocation; correct negative/positive physical origins; distinct subpixel bins, scale, DPI and resolved variation locations. The DPI pair keeps physical scale equal while changing pixels-per-point. Actual `wght=300..700` glyph-parity and allocation markers are present; the font was available and executed.

Review corrected three issues confined to the new test: a `Debug`-requiring assertion on the existing key type, an incorrect origin expectation, and a DPI case that also changed physical scale. No runtime key or behavior was modified to satisfy the test. A prelaunch target-write denial has no native result; a subsequent compile-only failure returned101 and ran no test bodies. Both attempts remain preserved. Final source formatting check passes.

| Evidence | SHA-256 |
| --- | --- |
| Candidate `vendor/epaint/src/text/font.rs` | `9530085eb7d8999afc4ae197ba45da90172d7a863c758a531be1744f73bf4e85` |
| `receipts/candidate-font-003.patch` | `b9a23caa9c091f94d9904f47182e8a6f8c55e01e183576addc06e695c55319ce` |
| `receipts/candidate-source-freeze-003.json` | `2aa3f0d431dffc71045d89f983a9121e273c72bed8ae6050d8c6a47e987e2f08` |
| Private harness lock | `fc8fa3053c4abb2e57aa82f1e7ef579c17da56261222a5bd8764d69a4c441b1c` |
| `receipts/private-candidate-003.log` | `5073bb0ddce0cff73dc68ba7c62f85ba86571b63460b70f7fddf2d54dddbe6f8` |
| `receipts/candidate-private-final-index-001.json` | `06e0c822e2ba583574a79ba2d56b0e7f1647068a29f71a19e074577ef94e7b4c` |
| Actual `C:/Windows/Fonts/Bahnschrift.ttf` | `bff1ca87ea35e376691498c618556411f0ee71784b32aa351e6e8f9e8bd609dd` |

Receipt paths are relative to the experiment directory. Parent and independent reviewer checked all ten actual test names, native result, source/diff/freeze/command/log hashes, actual font asset/markers and correct candidate library path. The combined source/private gate is accepted without remaining findings.

## Release verification

One candidate release archive was built in a fresh target, preserving the baseline. Focused input equality and source bindings were checked before compilation. Source/profile/archive/executable and exact-owner listing review preceded the candidate parity body, which passed both the profile-aware reference gate and exact release snapshot comparison.

The build launched at2026-10-14T05:19:52.1092482Z in owned session82541. Parent independently confirmed the named Cargo/Nextest processes live during compilation. Input binding `receipts/candidate-canonical-release-001-input-binding.json` (SHA-256 `181f9225adbb6fa85bfa4d8a0f0fdab7a577214ba7eeff881868c6219451449e`) retains baseline-equal manifest/lock/font inputs and actual default-font asset hashes; parent independently verified those hashes. Start receipt records frozen source, new target/archive paths, canonical release, locked/offline inputs and absent overrides/QA.

The single build completed native exit0 in723.227914seconds. Stderr reports the optimized release profile and one archived test binary. Archive `receipts/candidate-canonical-release-001.nextest.tar.zst` is30,591,923bytes, SHA-256 `0219a6bc487972e5bc3accc79ab6b837311d370a70048cd3c72279c1d09d877a`. Source font remains9530085E…BF4E85. Parent checked actual result/logs and rehashed source/archive.

No-body listing002 passed native exit0 in2.145041399868205seconds. Actual release/MSVC metadata lists one library suite,5,238 cases and the exact ignored owner. Built and extracted executables match:87,120,896bytes, SHA-256 `352c48eda64a62013c34b8228d38e3dbe7401e259fe315f8d4c6c740a11fd27b`. Binding `receipts/candidate-canonical-release-listing-002.binding.json` (SHA-256 `e7ccb6ab88b997d25a4a5cae6e05ceb1dafb9cd89fb7f92e1ebaa3c3f24ea25f`) and appended actual source-build path correction `candidate-canonical-release-listing-002.binary-path-correction-001.json` (SHA-256 `f6c215e60e8881cd5b5e7d427e81d5354608d20b5c61752fa0e96d0a1a378354`) were independently accepted without remaining findings.

The listing001 wrapper parse failure occurred before native execution and is preserved. Listing002's initial validator misread the JSON structure; its saved raw metadata was correct and was analyzed without a native rerun. The source-build path alias was corrected in an appended receipt after verifying the real build-target executable. These evidence-tool corrections did not change source or repeat a body.

Parent separately authorized exactly one candidate parity body. Its descriptive source marker is the candidate font-file SHA953…; receipts also bind unchanged renderC511 and the full accepted input/freeze chain, without claiming a Git commit or complete-tree hash. Full actual parity review preceded matched timing.

After actual parity review, a fresh baseline/candidate timing pair ran sequentially: one invocation per bound archive, two warmups/five samples each, all ordered vectors retained. Baseline archive was reused without rebuilding. The corrected explicit Python child environment removed QA keys and asserted absence. No exclusions or timing repeats.

Candidate parity passed native exit0 in2.4651184seconds. Its complete4,964,373-byte canonical snapshot (`candidate-parity-run-001/output/raw-hit-canonical-snapshot-001.txt`, SHA84BDA12C…8C444D) and8,632-byte classified diff (SHA075EF5B6…D07AA6) are byte-identical to the accepted release files. Parent and reviewer independently verified full bytes, all55 counters, work/identity tuple, source/archive/executable bindings and actual QA absence. Result `receipts/candidate-parity-run-001.result.json` is SHA-256 `9cd1b7bd66e17f5029b8b7c44f917a9fd491ea13d1932a32fa1007751f5f2d95`; actual parity review closed without findings.

Both timing invocations used the existing owner's `variant=candidate` control, which identifies the earlier raw-hit behavior; outer receipts distinguish the map experiment's `baseline_ahash` and `candidate_nohash` roles. Descriptive source markers bind each respective font file, with unchanged renderC511 and full source/input/archive chains retained. Each regenerated parity and preserved all eight vectors (parity, two warmups, five measured records).

## Matched result and decision

Baseline and candidate both passed with native exit0; process walls were3.5439509 and3.3602086seconds. These process envelopes include setup, extraction and canonical validation; they are not measured owner latency. Full snapshots/diffs are byte-identical to accepted release artifacts, and non-timing counters match across all16 records.

| Measured interval | Baseline p50 / range ms | Candidate p50 / range ms | Descriptive p50 delta |
| --- | ---: | ---: | ---: |
| Frame owner | 30.9320 / 28.3656–40.0341 | 35.3300 / 29.6846–44.9570 | +4.3980ms / +14.22% |
| Shaped-run emission | 9.2382 / 8.4288–11.6562 | 9.9939 / 8.6672–12.9393 | +0.7557ms / +8.18% |

Ordered owner nanoseconds:

- Baseline: `[39039600,30932000,40034100,28365600,29655500]`.
- Candidate: `[44957000,35330000,37335300,30572000,29684600]`.

Ordered emission nanoseconds:

- Baseline: `[11252500,9238200,11656200,8428800,8898300]`.
- Candidate: `[12939300,9993900,11093600,9080900,8667200]`.

Five-sample p50 is the third value of a separate sorted copy; raw arrays remain in execution order. Emission is nested within owner time and must not be added. Wide sample overlap and one sequential pair do not establish a causal regression, but the data demonstrate no supported benefit. Per the reviewed experiment plan, reject this candidate for production; do not repeat it to seek a better result.

Raw logs are `receipts/matched-release-{baseline_ahash,candidate_nohash}-001.{stdout,stderr}.txt`; complete snapshots are in `matched-release-timing-pair-001/{baseline_ahash,candidate_nohash}/output/`. Compact `receipts/matched-release-pair-index-001.json` has SHA-256 `ec14722fa2c6fd40593a155e42da3f13c057f6dc329639e6c25aee36f2bae630`. Parent and reviewer independently verified all16 vectors, order, medians/ranges, full snapshots, controls and source/archive/executable bindings. Matched evidence review closed without integrity findings.

The ignored prototype and its evidence remain preserved. Production adoption is not authorized and no additional optimization or measurement follows automatically. A separate bounded root-cause proposal is requested from the planner; full mission performance/native/renderer/theming gates remain required.
