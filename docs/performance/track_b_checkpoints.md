# Track B checkpoint ledger

Baseline: cargo-build-perf at 6a6be519685652a282f7d0a411008601f158d075.
Branch: build-optimization. No push/merge.

| Checkpoint | State | Evidence / findings | Reviewer | Local commit SHA |
| --- | --- | --- | --- | --- |
| B0-A | PASS | Clean initial HEAD; metadata and host inventory; actual MSVC14.44 linker verified | planner/parent | dfc325f0 |
| B0-B | PASS | Harness self-tests; exit 7, arguments, env/cwd, timing freshness; null target override fixed | approved; null target inference fixed | 2849632c |
| B0-C | PASS | Frozen launcher244.683s/all-bin323.159s; branch243.453/240.955s; broad943.249s;6389cases/17ignored; PE original verified | approved; attribution/roster collision findings resolved | this checkpoint commit |
| B1-A | pending |  |  | — |
| B1-B | pending |  |  | — |
| B1-C | pending |  |  | — |
| B2-A | pending |  |  | — |
| B2-B | pending |  |  | — |
| B2-C | pending |  |  | — |
| B2-D | pending |  |  | — |
| B3-A | pending |  |  | — |
| B3-B | pending |  |  | — |
| B3-C | pending |  |  | — |
| B3-D | pending |  |  | — |
| B4-A | pending |  |  | — |
| B4-B | pending |  |  | — |
| B4-C | pending |  |  | — |
| B4-D | pending |  |  | — |
| B4-E | pending |  |  | — |
| B4-F | pending |  |  | — |
| B5-A | pending |  |  | — |
| B5-B | pending |  |  | — |
| B5-C | pending |  |  | — |

## Execution state

B0 is frozen. All measured Cargo/Nextest commands succeeded. Old batch24321 exited1 only at final clean-tree assertion for test-generated scratch clipboard_modifiers.json; parent preserved/moved only verified owned data and checked23manifests/clean status. Old queue43257 terminated before branch work. Replacement queue86511 completed0, four measured branch switches/restoration and original PE icon verification. No active Cargo job remains. Raw manifests and fixtures live under ignored target/track-b; do not rerun completed baselines or delete caches.

Normalized inventory baseline-nextest-roster-final{,-summary}.json:72suites/6389cases/17ignored/2empty/zero logical duplicates. normalize-nextest-roster.ps1 rejects existing outputs with CreateNew; later parity compares suites only, preserving ignored/filter facts, excluding target paths/profile metadata. Reviewed final guard approved. B0 report review resolved exact causal attribution wording; differences are observed medians, not exact target-only savings.

B1-A next: implement only2explicit resource rerun directives while retaining embed_resource::compile options/link breadth. Parent creates benchmark-only candidate ref from B0 freeze, copies only build.rs, commits candidate solely for clean probe provenance, then executes reviewed ignored run-b1-resource-probes.ps1. Actual implementation checkpoint commit follows verified probes/review; B1-C uses accepted implementation SHA and matched small-edit/no-op workloads. Candidate/active production normalized content must match. No active branch switching/merge/reset.

B2 symbol-pair runner preparation assigned to implementer, ignored script only; no candidate manifest or Cargo execution yet. Prepared native startup helper is statically approved but actual startup NOT RUN. B2 requires controlled default/fast-dev edits, source-resolved backtrace/PDB evidence and restored passing assertion test. Canonical release/full-debug defaults remain unchanged. lto=false differs from off; iteration tradeoff must be explicit.

B4 jobs/linker decisions remain pending: broad owned linker/memory/paging observations warrant reassessment after reduced symbols; one jobs4 paired trial if pressure persists, alternative linker only for material residual cost. sccache unavailable; dependency pruning lacks expensive-dependency evidence; crate extraction normally deferred. B3 targets/filters must select actual nonzero cases; grouping conditional with explicit complete parity mapping.
