# Track A runtime baseline (pre-optimization)

Status: M0 setup in progress. No speedup claims. Timing rows below are NOT MEASURED until the production-operation fixtures run in M0-C.

## Source and host

- Authoritative source base: `02399cf0848de2b429dfd9fae80a141ce50d972d`.
- Branch: `performance-optimization`; the user confirmed HEAD as base and waived ZIP comparison.
- Windows NT 10.0.19045.0, x86_64, MSVC toolchain.
- Processor identifier: Intel64 Family 6 Model 158 Stepping 9, GenuineIntel; 8 logical processors.
- `rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6.
- `cargo-nextest 0.9.135 (610eefb88 2026-05-14)`.
- Profile: pending harness selection; debug/test timings must not be described as release GUI latency.
- Monitor layout/DPI/refresh, power state, CPU model name: NOT MEASURED. CIM queries were denied in the restricted execution context.
- Telemetry: existing `MULTI_LAUNCHER_PERF=1` process opt-in; synthetic data only.

## Baseline source behavior

The source inspection confirms full-note clones/alias sorting before unchanged panel checks, full-history preparation before count filtering, per-entry command enumeration, construction of all root/Quick Notes results, GUI-thread path traversal on changed Actions reload, and cached effect refresh before live sampling. These are structural observations, not timing measurements.

## Controlled comparisons

| Operation | Synthetic sizes | p50 / p95 / max | Counts | Status |
| --- | --- | --- | --- | --- |
| Note panel unchanged refresh | 100 / 1,000 / 5,000 | — | — | NOT MEASURED |
| History prepare, count 8 / 50 | 100 / 1,000 / 10,000 | — | — | NOT MEASURED |
| Root list / grid | 100 / 1,000 / 10,000 | — | — | NOT MEASURED |
| Quick Notes variable-height browse | 100 / 1,000 / 5,000 | — | — | NOT MEASURED |
| Index scan / Actions reload | small / 1k / 10k | — | — | NOT MEASURED |
| Halo / zoom moving / stationary | each / both | — | — | NOT MEASURED |
| Coordinate sample / HUD | HUD / effects / all-off | — | — | NOT MEASURED |

The fixture recipe, sample/warmup counts, exact commands, instrumented commit and raw bounded summaries will be added at M0-C, before optimizations. Data generation will be excluded from target timings. Missing native measurements will remain explicit.
