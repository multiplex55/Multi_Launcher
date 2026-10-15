# M2-A partial production raw-shaping reuse

Production source and final execution evidence are independently accepted, with no remaining findings. This is a partial correction, not M2-A performance acceptance.

## Implemented

`FontFace` now owns a raw-result LRU alongside the existing FIFO8 shape-plan cache. Only the proven fresh/cleared layout buffer lifecycle admits reuse. Actual variable instances, including face-tweak variations, unknown buffer provenance and unsupported/nonempty buffers bypass it. The key covers exact text and resolved script/direction/language/flags under the documented fixed shaping-options contract.

The owned result carrier exposes complete immutable glyph slices and recycles the original buffer on consuming clear. Glyph emission, fallback, spacing, raster behavior and plan selection remain unchanged. No application, dependency, font, renderer or profile changes; no prototype counters, clocks, modes or reset hooks.

Limits remain 128 LRU entries / 1 MiB retained key-and-glyph payload per face, 4 KiB text and 64 KiB per entry. Checked oversize/overflow rejection precedes eviction. Payload excludes allocator/shared-pointer overhead and temporary evicted shared handles; it is not total heap or RSS.

## Source and private review

Independent review closed two findings: the prepopulated-buffer fixture now deliberately claims clean provenance to exercise the nonempty-buffer guard independently; PATCH.md states the fixed features/scale/context/replacement contract. No runtime defect or additional source finding remained.

Accepted source SHA256:

- `vendor/epaint/src/text/font.rs`: `D31D7577AEC52AC58C42B0A37FCB71D40E7B8350E14FDA8B66B961C3DDD2F887`
- `vendor/epaint/src/text/text_layout.rs`: `0537FF82CD641E7BD866A47142612026F80A83133FD219674B1CC767B261A2E3`
- `vendor/epaint/PATCH.md`: `197D0EB6A5993E9BB4094EC6775463A96971BB645176CD1B037B5CCDC4AD836D`

## Verification so far

Raw receipts are under ignored `target/egui-modernization/m2a/production-adoption-validation/receipts/`; fresh private harness is `epaint-focused-005` and targets live vendor source.

- Private011: exact eight contracts passed, native exit 0, command 3.0467084 s. Four existing plan/font contracts and four raw-cache contracts retain full glyph comparisons, actual Bahnschrift 300/700 format/face-tweak checks, discriminatory policy references, allocation-pointer recycling, key/face isolation and admission/LRU bounds. Initial test compilation failures remain preserved.
- Root-focused20-002: exact 20 tests passed, native exit 0, command 176.8460936 s. List/grid geometry, font activation, Notes, query output and texture disposal are covered. Preserved root binary SHA256 `2CD4D2D4621029DC47498CC68EF03AA1E85FDADA05D47FC8CF318BFDC6F61E92`.
- Locked/offline all-target check002 passed, native exit 0, 78.104874 s before the final test/prose correction. Final-source check003 also passed, native exit 0, 30.1432523 s.
- Root/vendor formatting passed with direct stable toolchain binaries, native exit 0, 12.569623 / 0.2846063 s. Initial rustup-shim access-denied attempt is preserved. Diff whitespace check passed.

## Normal-preparation diagnostic

Unchanged root-geometry-cold-warm-002 passed, native exit 0, command 15.2609603 s / test 13.255 s. Parent independently parsed all 20 records: two warmups and five samples for each cell, no exclusions. Every record has 25 painted rows, one adapter pass, no geometry errors/abandonment and the expected output identity. Cold cases rebuild once and measure the full row count; warm cases perform no geometry work.

| Rows | Cold owner p50, ms | Warm owner p50, ms |
|---|---:|---:|
| 100 | 14.3379 | 2.1737 |
| 1,000 | 213.2354 | 2.1353 |

All five 1,000-row cold owner samples, ms: 213.2354, 219.6420, 212.1461, 214.3572, 210.6245. Warm samples: 2.1353, 2.2205, 2.1095, 2.8243, 2.0959. Output identities remain `6688701683edffbd` (100) / `bd4860c5d2427fa2` (1,000). Source attribution is final manifest005 SHA256 `A86C3205BB067DF688C2E0CA63E6DB10B5672EEA0CB146266D6A39825AE49C1D`; fast-dev and QA environment exclusions are explicit in the receipt.

Normal production caches participate during preparation; this does not reproduce the archive experiment's explicitly empty raw-cache start. These new samples are not a fresh matched original comparison. No full acceptance suite, release or native run belongs to this partial packet. Original performance parity remains unresolved.

## Final bindings

Exact executable argument vectors, environment and provenance are retained in `final-command-argv-provenance-001.json`, SHA256 `1CF3498875144F0DA440208EEF386D161A9CF198B4855432C5FA856D22F2E9F6`. The OR filters are each one argument. This additive receipt clarifies earlier rendered command lines; no commands were rerun for the receipt.

Parent independently verified all 15 final manifest rows against current file hashes/lengths, manifest/bundle hashes and both preserved test binaries. Final private binary SHA256 `AACD9A2A41CE9688F2D409773AA2C492B3D65F500FA3DD8D4FAB7A2CA0BC11D9`. Root diagnostic uses the same preserved root binary listed above. Source writer released without further edits.

Reviewer independently verified source, commands, binaries, all scoped execution results and all 20 diagnostic records, accepting partial integration with no remaining findings. Inherited diff-check003 listed an older manifest/PATCH scope; reviewer explicitly ran `git diff --check -- vendor/epaint/src/text/font.rs vendor/epaint/src/text/text_layout.rs vendor/epaint/PATCH.md`, exit 0. Parent's whole working diff check also passed. No extra build or samples were needed.
