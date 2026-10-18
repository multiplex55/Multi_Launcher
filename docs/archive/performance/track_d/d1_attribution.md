# D1-B attribution and candidate gate

Baseline source: cbe104abc10ff03af2fd9897e88a4a331a7f4197; measurement
checkpoint 030e4868. Rust/Cargo 1.99.0 MSVC x64, default dev, default features,
existing target cache. Exact commands/samples: d1_baseline.json. No release
comparison or cache removal. Cargo timing HTML retained with external raw runs.

| Owner / repeated path | History edit | GUI edit | Meaning |
|---|---:|---:|---|
| Application library check | 11.38 s | 11.20 s | Cargo unit duration |
| Application library build | 29.78 s | 33.21 s | 13.43 s frontend in both; 16.35/19.78 s codegen |
| Application binary build | 11.84 s | 12.78 s | Internal phase split unavailable |
| Selected lib-test build | 46.90 s | 45.89 s | Whole command, linker split unavailable |

The repeated critical path is the application library followed by binary work;
no dependencies rebuilt in the edited build timing reports. GUI edits did not
invalidate an additional Cargo unit compared with history edits. Module size
alone does not justify extraction. The 250.81 s preparation is a different cache
state; it is not a repeated edit-cycle baseline. The dev PDB observed before
the build was 756862976 bytes; size is context, not a linker-duration metric.

Candidate 1: remeasure the **existing** fast-dev reduced-symbol workflow. This
is a recipe comparison, not a new production optimization. Boundary: existing
Cargo.toml profile and developer_loop.ps1's profile argument; no product source
changes. Expected direction: less symbol/codegen cost. Absolute saving unknown
until measured. It must save >=15% AND >=10 s in both edited comparisons to
justify a current-checkout recommendation; no-op guard <=15% or <=2 s absolute.
Prepare its separate cache once, retain it, and keep preparation separate.
Run the exact selected library test as the adjacent functional guard. Budget:
55 minutes, one variant initially, two maximum. Existing iteration profile is
outside this comparison; canonical release behavior remains unchanged.

Candidate 2: SKIPPED_NO_CANDIDATE. No distinct small owner accounts for enough
of the remaining check/frontend time to justify a >=5 s /20% claim. A crate
split or broad dependency change would exceed this bounded task. Do not invent
another profile to duplicate fast-dev.

Rust Analyzer: one bounded headless history analysis observation recorded in
the ledger. Build scripts/proc macros disabled; this does not measure interactive
LSP diagnostics or editor response and cannot support a response-time claim.
