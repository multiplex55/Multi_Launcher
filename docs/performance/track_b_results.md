# Track B results

Baseline: cargo-build-perf at 6a6be519685652a282f7d0a411008601f158d075.
Implementation: build-optimization; local commits only, no push/merge.

B0 inventory complete; controlled baseline measurements in progress. No speedup claims yet.

| Scenario | Baseline | Final | Caveat |
| --- | --- | --- | --- |
| No-op release launcher | NOT MEASURED | NOT MEASURED | Must be fresh |
| Small source-edit release launcher | NOT MEASURED | NOT MEASURED | User-reported six-minute class |
| Large source-edit release launcher | NOT MEASURED | NOT MEASURED | Same library crate |
| Default all-bin release | NOT MEASURED | NOT MEASURED | Four binaries |
| Branch switch release | NOT MEASURED | NOT MEASURED | Isolated refs only |
| Targeted Nextest build | NOT MEASURED | NOT MEASURED | Select Cargo targets first |
| Broad Nextest discovery/build | NOT MEASURED | NOT MEASURED | Retain ignored cases |
| Iteration profile | NOT MEASURED | NOT MEASURED | New cache population separate |
