# M2-A production early raw-hit return

Status: scoped production adoption complete and independently accepted, with no unresolved findings. M2-A and original performance acceptance remain open.

## Change and ownership

`FontFace::shape_for_layout` now looks up an eligible retained raw result before constructing the HarfRust shaper or consulting the FIFO shape-plan cache. A hit returns the existing owned carrier with exact glyph arrays and buffer recycling. Misses and bypasses retain the native shaping path. Actual variation-instance exclusions, pre-mutation eligibility, complete keys, raw LRU limits, memory accounting and emission remain unchanged.

A raw hit intentionally no longer repopulates an evicted shape plan. Native shaping continues to use FIFO8. The ninth regression inserts real distinct public language keys to evict the original plan while retaining the raw entry, then verifies exact cached output, recycling and unchanged actual plan contents/order. The only helper visibility change is inside test code.

Production scope is `vendor/epaint/src/text/font.rs` and `vendor/epaint/PATCH.md`; no application, dependency, lockfile, font, profile, renderer or text-layout changes. No archive instrumentation enters production. PATCH records accepted experiment checkpoint `75a611d0`, the observer-overhead limitation, upstream provenance and removal condition. Independent runtime/test review found no defects; the requested evidence-commit documentation correction is closed.

## Source and artifact bindings

Verification binds uncommitted source explicitly to base HEAD `4380aae55c5ad7a80f6d337c57d674a4bc22f0e5` plus the two-file diff; it does not claim that base commit contains the change.

| Input | SHA256 |
| --- | --- |
| Live font.rs | `8EAE76FC33F858E2AC977ED3571D0E7DE4024F3E1A1B0A590999DFAC96FD5517` |
| PATCH.md | `5ABD9F326CB8E95429EEBFC3E76FA76AAD61A0CEA2EE94D9FA62D98BA425CF0E` |
| Two-file diff | `69C82447F49D33E2618BB4C1C29ACEA640E0AB44EA4975684130419E157DC207` |
| Private binary | `4E2AD5BD196945EAD7882F331CE6DC7A785F7A9CA08B5F4E2A96918A93BDE341` |
| ROOT Nextest archive | `BA291A3CC7D446BD3F6B3ECD3BADDE48EA2E0399708C7EA85D95A53A38D6DD0E` |
| ROOT current/extracted binary | `C47778065AD06FFBC63DDA0C66C6C2E15C85D66E9E900909D12A79E1E60BF661` |
| Source/private/ROOT index001 | `B98F6D6D2484B8D9D460BC4BA7AFB529F4277C0F8A04DAF61B64B1F2D80CA230` |
| Completion addendum001 | `9BCF8D58C8F35DB1C511FA94BEE4F4A4BCD6CAA4AC8B6F404EC8719F30295D49` |

Local evidence is under ignored `target/egui-modernization/m2a/production-raw-hit-adoption/`. Builds use a fresh isolated target, stable Windows Rust/Cargo1.99.0, locked/offline fast-dev and explicit working directories. The shared target remains intact. ROOT archive metadata lists all20 requested tests and the unchanged geometry diagnostic among5,236 available cases; current/extracted executables match at138,978,304bytes.

## Verification so far

- Private no-run compilation passed, native0/29.235717seconds. The fresh pinned harness resolves to live vendor source; full listing contains the intended nine among54 available tests.
- Exactly nine private contracts passed through explicitly exact frozen-binary libtest invocations. Each reports native0, one passed and53 filtered out. Before/after source and executable hashes match. Actual Bahnschrift300..700 raw-cache variation/face-tweak and shape-plan complete-glyph markers passed.
- ROOT build/archive passed without test bodies, native0/329.5102297seconds. Fresh extraction and listing passed; independent artifact review accepted the complete binding without findings before ROOT execution.
- Locked/offline fast-dev all-target check passed, native0/178.7561374seconds. Direct vendor rustfmt with edition2024 and scoped diff checks passed. An initial wrapper preflight failure is preserved as a no-command receipt; it is not counted as a compiler or formatting result.

ROOT20 passed20/20 through the reviewed immutable archive (native0/10.8163615seconds). The unchanged geometry diagnostic passed1/1 with all20 records (native0/16.4117121seconds). Both extracted executable hashes matched the reviewed artifact; source and archive hashes remained unchanged. The diagnostic labels its source with binding-manifest hash `B98F6D6D…CA230`, explicitly not a Git commit; its receipt separately records base HEAD and actual source hashes. The invocation's002 suffix and output directory's001 suffix are independent names; no earlier ROOT20 invocation or failure occurred.

| Rows | Cold owner median ms | Warm owner median ms |
| --- | ---: | ---: |
| 100 | 16.1288 | 3.5598 |
| 1,000 | 202.9620 | 2.2417 |

All20 samples retain the expected identities,25 painted rows and one adapter pass. Cold records have one rebuild and their item count in measured rows; warm records have no rebuild/measured rows. Geometry phase errors and abandoned counts remain zero. All five samples per cell remain included, including the slower100-row observations. Parent and independent reviewer audited the actual records and accepted all scoped criteria without unresolved findings.

Final execution index001 SHA256 is `0D19D30C1E33A21F9D479A2FECB3A546A2636E442F833C789E40293527B82EC5`, binding44 verified artifacts. Its null base-HEAD field is clarified by unchanged-evidence addendum `final-execution-index-001.base-head-correction.json`, SHA256 `184A525203CF21DA217E95E0A94A27B7366CFAF8E63DE7955D0BD0387928C018`; underlying source/execution receipts already record the correct precommit HEAD. No source changes or reruns followed review.

This partial adoption does not quantify production savings or establish original, release, native or whole-application parity. These medians are not a matched before/after comparison. The archive experiment's227.9514→217.8085ms includes skipped observer work and is not a production latency claim.
