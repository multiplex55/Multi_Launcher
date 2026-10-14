# M2-A ASCII face-resolution storage candidate

Status: source/private correctness and canonical ROOT artifact/listing accepted. The single comparison completed; candidate held without adoption after independent result review. Production796/Full ROOT FAIL unchanged. Plan9ecfb7d3; fresh ignored ascii-face-resolution-001/root copied from frozen796/4cf, separate from the warm guard copy.

CachedFamily owns exclusive ASCII [Option<FontFaceKey>;128] and non-ASCII map storage. Central helpers route existing Font::resolve_face reads/writes; ordered resolver/fallback, negativeSome(INVALID), both constructors, lifetime and reconstruction semantics remain intact. No shaping, segmentation, glyph allocation, layout, raw-result policy, dependencies or timers change. One private contract covers128ASCII cold/repeat against ordered lookup, exclusive containers, actual emoji fallback, empty negative caching and reconstruction. Actual fixed array cost is 2,048 bytes per family; do not assume map savings.

Only fonts.rs SHA `1cde6124f28c1b813283c2fa85ace9ed29c1379073816746389703596dc84adb` and font.rs SHA `acc8b2ccaecbc9ac06a088a64001e51edf24ca2536f63667face14b04ee148bf` differ. Patch `50448df677999e90ef473c8751b56ab67e114d4ef671c51d11aa309723105784`; provenance `e88784715b34b461289fdcabd53f7b3a4a6007f84548fa6cf07c1e1f9c17c943`, under candidate receipts. Scopedformat0, CRLF preserved. Parent inspected exact patch and receipt hashes; reviewer accepted source/harness without findings.

Private harness manifest `ef241bb288e4d0e59aa5d232c4b1bc3076d4b7b01d2e498ae720169707bbc7b5` differs only in [lib].path pointing at this candidate; pinned lock `fc8fa3053c4abb2e57aa82f1e7ef579c17da56261222a5bd8764d69a4c441b1c` remains identical.

Next: exactly four targeted Nextest fast-dev library tests (new resolver contract; unchanged full_layout_matches_frozen_stock_output, positioned_row_measurement_matches_full_size_bits and test_fallback_glyph_width), fresh private target. Parent owns native execution. No ROOT/performance clearance or production adoption follows from private correctness.


## Private correctness evidence accepted

All four approved unique tests passed across two Nextest runs; the initial failure is preserved. First run75131829-e96e-459a-9cde-16fc78fee99d (private-tests-001) ended100/29.0437735s: resolver contract and fallback-width passed, stock full-output failed, size parity did not run because of fail-fast. Resolver stdout reports actual array storage2,048bytes per family; no map-saving offset is assumed. Result SHA `7be85188a10628bef56b0e43afadc5f90fe7734c8af2c79755b2bae31a978ffd`, stdout0E8350CF35AE86871FBBFE9BC7A67B2CD2EAD4AD237CC2B0B730FA677F19C2B4/stderrCB767E0B0D076CF4D246F6CD210B4CD9A84FA6BBA230784CDF50E2028B978F81.

Parent decoded the failed mixed_sections strings:24,400actualbytes versus24,598expected; equality holds exactly after replacing CRLF withLF. The copied frozen archive had converted the two serialized fixtures toCRLF. Candidate fixtures were restored from immutable796 Git blobs, not generated from candidate output; old copies and exact normalization proof remain in fixture-input-repair-001. Canonical D75C7B2C...FADD51D and3AB2A7AC...5F5E2B7 match existing production fixtures. Runtime source/harness/lock remain unchanged. Candidate now differs from copied frozen root in two runtime files plus those two fixture EOL restorations; frozen baseline/live files were untouched.

Only failed stock-output and previously unrun size-parity were rechecked on the same target with no-fail-fast: runfebbc871-60f7-4889-96b4-cefbcafb6858 (private-tests-002), native0/3.0750987s,2passed/58skipped. Result SHA `67a866e9797a5a9b8e37cc1514fbddfdb59bdf61496df5e642eee021a64e28fd`, input315E2420D4F299FB70D4BA7069368C3AD84CAC2F9F48528851EDA17BD84606EF; stdout1D59CF1032440E377768F24A00E06C6A1D1DB0CE5755BAC3BAE32F3FAF5E1BC4/stderr801D143575239ABE74405AFE251CA76A3C8D68DBDF7CB1E3E46A9B4C240E798A. Both stock jobs and exact measured-size parity pass with untouched canonical expectations. Two previously passed tests were not repeated.

Independent private-evidence review accepted without findings: four tests, actual2048Bcost, unchanged runtime/harness bindings and exact canonical fixture restoration verified. Source/private milestone complete only; no ROOT/performance/adoption clearance. Future archived private testing must preserve these serialized fixtures' exact Git bytes rather than normalizing their line endings with Rust source.


## ROOT artifact preparation started

Reviewed experiment plan73f1e91b authorizes one canonical candidate artifact/listing, then a separately cleared single contemporaneousF26/current796 baseline and ASCII pair. Candidate source excludes live guards6527ddc1 and warm instrumentation. Native Cargo clean in root-artifact-001 aborted101 for missing CACHEDIR.TAG before deletion; all preservedF26/C60C/A438 copies remained exact. No marker bypass or cleanup retry.

Independent process clarification accepted timestamp invalidation: only candidate src/lib.rs and vendor/epaint/src/lib.rs mtimes updated, byte hashes unchanged and recorded. Root-artifact-002/input.json SHA `edea39562c10673a913f917ccd25cbe0a4c7a6311f729fd84789fa00f2f99485`; canonical no-run started2026-10-14T16:05:37.293288Z, Cargo30276, parent-owned session42603, reused resolvedtarget/lockedoffline/release/defaultfeatures/cleanenv. Parent retained the completion handle. Artifact gate must require both candidate epaint and root Fresh=false with correct paths/profile; no benchmark body is yet cleared.

Candidate protocol identity is the honest whitespace-free base796+ascii-patch50448 full label, separately bound as patched_frozen_root rather than a Git commit. Actual comparison still pending; no performance or adoption claim.


ASCII canonical artifact/listing completed (review pending): parent-owned build30276 native0/444.9987014s; resultd577c91d7d07acb620469a737e3404c9ce540909dcc42d082a74da130244a1e0. Cargo confirms epaint and root fresh=false at the ASCII source paths; root opt3/debug0/assertfalse/overflowfalse/testtrue/features[]. Preserved candidate29f5d66505c88ca1bb79d5cea152d00d5fbcf516f0a40978ce07f0918d95e9e3/86,799,360bytes. Exactly one Full-owner no-body listing native0, result75733065fd53d759fcc6d3d5ef8db73eb5c6438147292be3d4afed63ceb88645. Source/pins/fixtures and F26D/C60C/A438 copies unchanged. Evidence ascii-face-resolution-001/receipts/root-artifact-002; no timing body/performance/adoption clearance. Independent completed-artifact review requested before local checkpoint and separately activated single baseline-first/candidate pair. Live652 production qualification and both mission tracks remain pending.

Independent completed-artifact review accepted without findings: fresh epaint/root, release profile, source/pins/fonts/copies and one no-body listing verified. Artifact checkpoint precedes the separately activated single baseline-first/candidate comparison; no performance/adoption clearance.


## Single contemporaneous Full comparison

After artifact checkpoint `575ab68a`, parent executed exactly one baseline F26D/source796 body, then one ASCII29F5 body. Native exits were0; wall times38.0079024s and37.8033964s. No repeat, exclusion, concurrent compiler or source change. Direct logs and native receipts are in `ascii-face-resolution-001/body-runs/full-baseline-001` and `full-candidate-001`; offline index `matched-full-ascii-001.json` SHA `4eb82384470deedabe6d6d9cf1d869c06234b5946c4024a83f30d2508f9e736d` retains all12records/240timings/seven aligned vectors. Existing parser checks passed; all cross-arm output identities, revision/viewport receipts and work vectors match exactly. Every vector is stable within its case,25paint, cold one rebuild/allrows measured and warm zero geometry work. Source and executables unchanged.

Times in milliseconds; quantiles use sorted copies, p50 index9/p95 index18.

| Rows/state | Baseline p50/p95 | ASCII p50/p95 |
| --- | --- | --- |
|100 cold|1.2972 / 1.3781|1.2351 / 1.5172|
|100 warm|0.2662 / 0.3696|0.2766 / 0.3142|
|1,000 cold|16.0523 / 18.6573|15.3893 / 17.5425|
|1,000 warm|0.2954 / 0.4099|0.3114 / 0.3859|
|10,000 cold|163.5074 / 183.4941|160.0559 / 171.7980|
|10,000 warm|0.3238 / 0.4865|0.3659 / 0.4611|

Parent disposition: HOLD, no adoption. Cold medians improve modestly (~4.1% at1k/~2.1% at10k), with overlapping distributions; warm medians rise in all cases (~13% at10k). One fixed baseline-first pair and host variation do not establish causal lookup savings or a reproducible new warm regression. Evidence does not justify the2,048bytes/family cost or resolve original-version parity. Preserve the candidate and all failures/evidence, do not repeat for a better outcome. Independent result review accepted (below). Actual production652 GUI guards still need their own artifact/contract; latest production Full remainsFAIL. Planner is investigating the shared measurement materialization boundary read-only, without new source or body activation.

Independent actual ASCII comparison review accepted without findings: both native0, all12records/240timings/sevenvectors, quantiles/bindings/exact crossarm output and work verified. HOLD/noadoption accepted: small cold gains relative to spread, all warm medians higher, fixed baseline-first causality limits and2KiB/family cost. No repeat, port or performance waiver. Production FullFAIL and both mission tracks remain. Parent saves evidence checkpoint; planner prepares a bounded shared measurement-materialization packet read-only.
