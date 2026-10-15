# M2-A earlier raw-hit setup candidate

Status: complete, independently accepted bounded archive evidence. Production remains at `9b7006cd`; no performance gate or production adoption is claimed.

## Scope and source

The candidate returns an eligible retained raw shaping result before constructing the unused HarfRust shaper and consulting the shape-plan cache. Misses and bypasses retain the existing path. Raw results, instance exclusions, admission limits, owned carriers and buffer recycling remain unchanged. A raw hit intentionally no longer repopulates an evicted shape plan.

Both ignored roots derive from accepted post-section final013. The common ROOT addition is the ignored `track_c_diagnostic_root_list_raw_hit_precedes_shaper_setup` owner, with normal nine-pass preparation and raw reuse active. Post-section clocks remain off; no new instrumentation was added. Its 55-value frozen diagnostic vector is printed after timing. Existing plan-lookup observer work is also skipped on candidate hits, so diagnostic savings cannot independently quantify production latency savings.

Current source bindings:

| Input | SHA256 |
| --- | --- |
| Both ROOT files | `917C0C0DD362BDCED64159DA4866BCCAE3AFEFC4E6B0292604148F93A6C8421E` |
| Baseline font | `E6653C1375294BF68F8B7F7C7DF4172CEE54B04BC96189A920B7A244E149EA62` |
| Candidate font | `EBE67DF3568BF05E8D9080EF7D2BAB82C8A0B420DEE03F9E9DC7FA86C7CE047D` |
| Source freeze004 | `1958626ED3318E8BD80119892D4DBC2DBC954E3F9D1AD36F075F8613F0422BF2` |
| Source inventory003 | `DBF97454FF8E90A1CF71E7F1A15A9040B61199FD349984A73D49DA0E340113E3` |

Parent independently rehashed all 1,070 files in each root against the inventory, with no length or hash mismatches. Independent review confirmed the entire ROOT diff is exactly 240 additions and zero removals, solely the new owner; removing it restores final013 verbatim. Baseline and candidate ROOT files are byte-identical.

## Private verification

Baseline003 passed all eight existing contracts (native exit 0, 5.7737506 seconds). Candidate001 passed the same eight and the new retained-raw-result/FIFO-eviction contract (native exit 0, 5.9409202 seconds). Actual Bahnschrift variation and face-tweak checks passed. The ninth fixture uses real distinct plans and FIFO eviction, verifies the raw result survives, and checks unchanged real plan contents after the cached return.

Independent review verified the actual logs, font/harness/lock hashes and variation markers. ROOT-only logging corrections did not change these inputs.

## Artifact corrections and preserved failures

All experiment artifacts are under ignored `target/egui-modernization/m2a/raw-hit-setup-candidate/`. Failed records remain preserved and are excluded from successful evidence.

- Initial baseline private log redirection failed before native exit capture; the originally inferred stale zero exit was invalidated. Baseline002 then failed because a Nextest profile was confused with a Cargo profile. Baseline003 is the actual eight-test pass.
- Shared-target baseline001, candidate001 and correct-working-directory baseline002 archives all packed the old post-section executable `1739C75D…C4865`, without the new owner. Native-zero archive returns and correct metadata package IDs did not establish a source binding. Full archived listings caught this before any ROOT test body ran.
- Initial archive/list receipts misstated the shell working directory; an append-only correction records the actual repository-root directory. Correcting the directory alone did not resolve Cargo reporting the old executable as fresh.
- Nextest extraction requires an existing empty destination. The initial missing-directory failure and unsupported listing-type attempt are preserved.
- A package-scoped Cargo clean dry run refused the shared target's missing cache safety tag before deletion. No tag repair or target cleanup was performed. Separate fresh per-version targets were authorized instead.
- The first genuine isolated baseline compile failed with six format-string errors caused by logging edits accidentally reaching historical diagnostic owners. Those strings were restored exactly from final013. Source freeze004 and full-diff review supersede freeze003. A receipt addendum corrects an initial inaccurate 232-line diff claim to 240 additions.

## Immutable artifacts

Builds use Windows stable Rust/Cargo 1.99.0, locked/offline `fast-dev`, explicit variant working directories and separate isolated targets. Archive builds execute no test bodies; listing and extraction establish the exact ignored owner before execution.

Baseline004 built and archived successfully (native exit 0, 171.360230 seconds). Archive SHA256 is `AE330FC250EFB84BA96702BD35ABBD9D544829C91E9B993F7A42912E01FCA98C`; current and extracted executable SHA256 is `351F9258B73BA5E345B7E47E4D3629EFE098D7F9A577283651662A33146210C9`, length 139,197,952 bytes. The full listing contains 5,238 tests including the exact ignored owner. Artifact binding004 SHA256 is `F20B017AF39E9CB69ECAB73BD5A2117236D55EE58D8C76F272907B9C461F984B`. Independent review accepted these bindings and the private results without findings.

Candidate002 built and archived successfully (native exit 0, 332.816994 seconds). Archive SHA256 is `6D6D9B4500BC6617C6BA2E2D7BC2D7DEAA7D710B5EAD323C39A8B208EEF4D182`; current and extracted executable SHA256 is `B938E08430D58B7ACA7DBFD57816E8FC8D340411B630D0561447A8B82BB291C2`, length 139,197,952 bytes. The exact owner is present among 5,238 cases. Candidate artifact binding002 SHA256 is `7CD3426EBC38A407261BB23902E92A5B30014E42FA93AB8DFB175967ABA136DD`; paired binding002 is `9261C7655C7258D5F26C6B718463DDB1C455C418DC60704D7EDFBBCA31C2381D`. The preceding paired index mislabeled the source-freeze hash as a source-file hash; its preserved replacement corrects only that index field.

Independent review accepted both immutable artifacts and private/source bindings without findings. Parity and separately authorized timing used the reviewed archives, with distinct fresh extraction/output directories. No Full81, release, native, production adoption or subsequent optimization is included in this packet.

## Actual paired parity

Both parity-only invocations passed the exact ignored owner, with native exit 0 (baseline 4.266576 seconds; candidate 4.568951 seconds). Extracted executable hashes matched the reviewed archives. Parent and reviewer independently compared every byte of both 4,964,903-byte snapshots against the reference: all three are identical at `66D25A29A4A62B08A8E12E83CB7204012FEA7C65F2F1528DBC756563E740712C`.

Both have 4,047 ordered runs, 103,830 glyphs, one rebuild, 1,000 measured rows, 25 painted rows and one retained pass. Raw-cache fields match: 4,047 lookups, 3,072 hits, 975 misses, zero bypasses, 975 insertions/evictions, 1,070,328 cumulative copied bytes, maximum 128 entries/175,833 payload bytes/14,336 shallow metadata bytes per face. Plan lookups/hits intentionally change from 4,047 to 975 (equal to native shaping calls); maximum plan entries change from two to one. All post-section clocks remain zero. Trace-stage times are not a timing series.

Independent review accepted the actual files, all 55 counters and command/result bindings without findings. Comparison002 SHA256 is `0F33F5854078F9D45E8CBE33F45EB8B3C737F07C7749576C8EBD2BC6853DA69F`. Parent then separately authorized one timing invocation per version, each regenerating parity before two warmups and five samples on the unchanged frozen archives.

## Retained timing results

Both timing invocations passed with native exit 0 (baseline 10.753454 seconds; candidate 9.806516 seconds). Each regenerated full parity, then retained two warmups and five samples. All ten samples retain the accepted identity tuple and matching work/raw-cache bounds. Parent and independent reviewer audited the actual logs, complete snapshots, vectors, all 27 artifact bindings and computed medians; no unresolved findings remain.

| Sample | Baseline owner ms | Candidate owner ms |
| --- | ---: | ---: |
| 0 | 227.9514 | 219.3547 |
| 1 | 227.1120 | 217.0857 |
| 2 | 235.0030 | 217.8085 |
| 3 | 235.0482 | 213.6547 |
| 4 | 226.4663 | 220.0647 |
| Median | 227.9514 | 217.8085 |

The diagnostic owner median decreases by 10.1429 ms (about 4.45%). Existing nested stage medians in ms are section 178.5061→168.7357, segmentation 27.4653→27.4562, native shaping 48.2671→48.8279, shaped-run emission 61.7998→60.9991 and plan lookup 2.4812→0.8655. These nested intervals must not be summed as separate owners. Plan lookups remain 4,047 versus 975 in every sample; maximum plan entries remain two versus one. No samples were excluded.

Timing index001 SHA256 is `7965DE91EE341781F36DBAB1115CF3F712364ED2F5DF448E292CC5B82764D31C`. Individual records and the table above preserve execution order. Its aggregate `owner_elapsed_sample_ns` arrays are sorted; append-only `paired-timing-index-sample-order-correction-001.json` SHA256 `35594692BCB746DC6E9AB1DC60CD6E2B3BB080C21EA8315E0C1639C94494019D` labels that representation and provides ordered arrays. The reviewer accepted the correction without changing or rerunning evidence.

This establishes a bounded diagnostic improvement for avoiding unused hit-path setup, including reduced existing observer work. It does not establish production latency improvement, original-version parity, release performance, native behavior or the M2-A gate. Production adoption requires a separate reviewed packet.
