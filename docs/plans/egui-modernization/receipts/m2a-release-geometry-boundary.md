# M2-A release geometry boundary

Status: existing diagnostic completed and independently accepted without findings. Performance remains FAIL/open. No production changes.

The canonical-release Full comparison at `a6a02dba` established a remaining ROOT regression. This measurement reuses the accepted current release executable from production `3a10f1de6a1f6be5093d3bcf4be664e32d5b3f76` to locate current cold-frame cost. It neither repeats the original benchmark nor compares historical medians.

## Execution

Preserved executable: `target/egui-modernization/m2a/release-full-root-control-001/receipts/current-release-multi_launcher-libtest-001.exe`, SHA-256 `2339e81d937e812285b16ff0f3af7e200d7d6c5c5725bc58891a85a8697123a9`. Existing source/profile gate `fa7db37a1c721eb5e74fb3bfc752f7a235c6251003d2f0934bc64b3b1b0c6c52` remains authoritative. No source, compiler, profile, dependency, asset or observer changes.

Fresh exact ignored-owner listing passed with one owner, native exit0 and43.462ms wall. Parent independently rehashed the executable before clearing exactly one body:

```text
<bound-exe> --exact gui::render::tests::track_c_diagnostic_root_list_geometry_cost_breakdown --ignored --test-threads 1 --no-capture
```

Frozen cwd is `release-full-root-control-001/source/current`. Child controls are `MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`, `ML_TRACK_C_PROFILE=release`, and actual source SHA above; QA and benchmark-mode variables are absent. The existing owner fixes100/1000 rows, cold/warm, two warmups and five measured samples per cell. Body passed1/1, native exit0, wall2.646334s. Executable and accepted source-audit receipt hashes remain unchanged. No exclusions or reruns.

## Paired measurements

All20 measured records are retained in execution order. Rebuild fractions and residuals below are calculated within each sample, never by subtracting unrelated medians.

| Rows / state | Owner p50 ms | Rebuild p50 ms | Paired rebuild / owner range | Paired residual range ms |
| --- | ---: | ---: | ---: | ---: |
| 100 cold | 2.2088 | 1.8961 | 81.6290–87.7578% | 0.3127–0.3848 |
| 100 warm | 0.3216 | 0 | 0% | 0.3013–0.3583 |
| 1,000 cold | 27.3130 | 26.9802 | 98.6777–98.7815% | 0.3328–0.3579 |
| 1,000 warm | 0.3113 | 0 | 0% | 0.2983–0.4069 |

For1,000 cold rows, ordered owner nanoseconds are `[27545900,27313000,27066500,26807700,27414200]`; paired rebuild nanoseconds are `[27188800,26980200,26708600,26461800,27065700]`. This establishes that almost all current large cold-frame time lies inside `RootListGeometryCache::ensure` rebuilding. It does not identify segmentation, native shaping, emission or another nested operation as the cause of the original-to-current regression.

Every cold sample rebuilds once and measuresN rows; every warm sample rebuilds/measures zero. All paint25 rows with one retained adapter pass, stable per-size output identities and zero geometry errors/abandonments. Actual release identities are `4438a3d6deccc7f8` for100 and `adfb11ee80438eab` for1000. Warm `stored_last_rebuild_ns` belongs to preparation; its current `last_rebuild_ns` is zero and no prior duration is subtracted. Geometry phase snapshots are integrity fields, not extra independent intervals to sum.

## Evidence and limits

Ignored root: `target/egui-modernization/m2a/release-full-root-control-001/runs/`. Listing evidence is `geometry-boundary-listing-001/`; body receipts and complete logs are `geometry-boundary-body-001/`. Its `geometry-boundary-results-001.json` retains all20 pairs and bindings, SHA-256 `b512df2ffbb1ea97be99c589d6728553a2ecbc73ea86004165506293442e194e`. Source audit rechecked all1072 extracted files against accepted inventory002. A listing-wrapper prelaunch failure is retained; it invoked no native command. A message-only inventory hash transcription was corrected in an appended note; the preserved source audit already contained the correct hash, and no receipt or run was replaced.

Parent and independent reviewer verified all20 ordered raw records, paired arithmetic, source/profile attribution, work, IDs, outcomes and receipt/index bindings without findings. This small run does not characterize run-to-run noise, prove performance parity, or explain the warm-path regression. M2-A and both mission tracks remain required. Any finer release attribution or optimization needs an evidence-backed bounded proposal.
