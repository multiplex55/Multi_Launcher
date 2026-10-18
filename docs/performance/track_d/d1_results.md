# D1 developer-loop findings

Original baseline cbe104abc10ff03af2fd9897e88a4a331a7f4197; baseline
measurements at 6e945b88; profile comparison at fe84cd10. Production Rust,
Cargo targets and profiles unchanged. This confirms an existing opt-in recipe;
it is not a newly implemented compiler optimization.

| App-build workload | dev | existing fast-dev | Saving |
|---|---:|---:|---:|
| History comment edit | 42.706 s | 27.655 s | 15.052 s / 35.24% |
| GUI comment edit | 47.613 s | 26.823 s | 20.790 s / 43.66% |
| Median of the two edits | 45.160 s | 27.239 s | 17.921 s / 39.68% |
| No-op median (3 each) | 1.125 s | 1.354 s | **0.229 s slower / 20.32%** |

Both edited comparisons meet >=15% AND >=10 s. This is one leaf and one large
owner replay, not a statistical population estimate. No-op exceeds the relative
15% guard but only by 0.229 s absolute; the measured 15–21 s recurring edit
saving outweighs that explicit tradeoff. Keep this opt-in recipe for work that
does not need full variable debug information. Keep ordinary dev for full-symbol
debugging. The separate fast-dev cache cost is real: its one-time preparation
took 234.33 s on this host and is excluded from edited gains. Default dev initial
preparation was 250.81 s under a different pre-existing cache, **not a matched
cold-build comparison**. Neither cache was deleted.

Commands:

```powershell
cargo check --lib
cargo build --profile fast-dev --bin multi_launcher
cargo nextest run --lib --cargo-profile fast-dev -E 'test(=history::tests::pin_roundtrip_and_toggle)'
```

Plain check remains the first feedback step: edited wall 12.34/12.26 s, no-op
median 0.972 s. Selected dev test-build edits cost 46.90/45.89 s; the selected
test itself passed in 0.028 s (Nextest run 0.039 s). Filtering the case does not
avoid compiling its library test binary. The restoration rebuild in the final
test command is separately acknowledged in the baseline ledger.

Fast-dev selected-test guard: PASS, exact history::tests::pin_roundtrip_and_toggle; case 0.032 s, Nextest run 0.050 s. First test-cache preparation compiled for 3m 18s; this is not an edited test speedup claim.

No second candidate: repeated application frontend/codegen dominates, but no
distinct small architectural change is established. Existing iteration already
addresses release-like iteration; no release campaign or new profile warranted.
No claim of isolated link/PDB improvement: Cargo timing units do not establish
that split. Fast-dev executable SHA256 after edited samples:
`eb12cd44cc8f71f4dfc1a5546d233a90178ebb8bbd6ca1c8650d4bd1fc962f25`.
Its PDB was 336908288 bytes; size is descriptive, not a latency measurement.

RA headless history observation loaded its database in 25.94 s, then hit the
60 s bound before analysis finished. Build scripts and proc macros were disabled.
Interactive/LSP responsiveness is unmeasured. No RA configuration was changed.

Reproduce selected trials with `dev/perf_track_d/developer_loop.ps1`, keeping
preparation/no-op/edits separate. Summarization requires explicit UTC bounds;
it groups by measured source SHA and complete command to avoid stale-session
mixing. Raw runs are external; sanitized per-sample records are in the adjacent
JSON summaries. All source edits were byte-restored and hash-checked.

