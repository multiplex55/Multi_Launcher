# Track B checkpoint ledger

Baseline: cargo-build-perf at 6a6be519685652a282f7d0a411008601f158d075.
Branch: build-optimization. No push/merge.

| Checkpoint | State | Evidence / findings | Reviewer | Local commit SHA |
| --- | --- | --- | --- | --- |
| B0-A | PASS | Clean initial HEAD; metadata and host inventory; actual MSVC14.44 linker verified | planner/parent | dfc325f0 |
| B0-B | PASS | Harness self-tests; exit 7, arguments, env/cwd, timing freshness; null target override fixed | approved; null target inference fixed | 2849632c |
| B0-C | PASS | Frozen launcher244.683s/all-bin323.159s; branch243.453/240.955s; broad943.249s;6389cases/17ignored; PE original verified | approved; attribution/roster collision findings resolved | 3b6e4af8 |
| B1-A | PASS | Eight invalidation checks; alternate/original canonical release PE payloads verified; fixtures clean | source/generator approved | ba3f7f59 |
| B1-B | PASS | Four target presets/docs; metadata78; four-bincheck28.668s; help/argv/exit/cwd checks passed | approved | 73fce728 |
| B1-C | PASS | no-op1.015507s; edited258.432568s (+5.62%, cause unresolved); all8commands0/clean restored | approved; no speedup claim | pending commit |
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

B0 is frozen at3b6e4af8. The baseline batch's23measurement commands passed; its final clean-tree assertion found newly generated clipboard_modifiers.json. Only that verified owned file was preserved/moved, then restoration verified. Replacement branch queue86511 completed0. Original PE icon verified. No baseline cache was deleted.

B1-A committedba3f7f59: exactly two resource watches; unchanged embed_resource options/link breadth. Candidate615bb025 contained the same production change. Eight check probes passed. Initial49199 stopped before icon builds on malformed generated ICO metadata, with source restored. Reviewer-approved fixture correction and guarded resume35390 passed alternate/original release PE payload checks (247.335619/244.137434s). Raw evidence remains in b1-resource-probes-1 and b1-resource-probes-icon-resume-1.

B1-B committed73fce728: wrapper/docs approved, actual metadata78targets/fourbins and combined bin check28.667616s passed. Wrapper argv/help/nonzero/cwd checks passed. No manifest/default-run/runtime/editor changes.

B1-C runner94284 terminated0; all eight commands passed and original color hash/Git clean restored. Accepted source73fce728; evidence b1-matched-1. No-op median1.015507s; edited257.320716/259.544420s, median258.432568 (+5.62% versus B0, cause unresolved). Resource script stayed fresh; same release flags verified by independent review, no correctness findings. Transition17.966304s allFresh and restores239.838158/244.770315 excluded. Exact linker duration NOT MEASURED. No active Cargo job; B2 source candidate may proceed after local B1-C commit.
B2 symbol-pair support runner is statically approved after Values-map, original artifact preparation, fixture-hash, automatic Args, and exact RGB failure-gate fixes. Parser/synthetic checks passed; actual profile execution remains NOT RUN. Native startup helper statically approved, NOT RUN. No profiles installed; default full debug and canonical release unchanged. Iteration support runner preparation is isolated to ignored evidence; lto="off" differs from default lto=false.

Normalized frozen baseline roster baseline-nextest-roster-final{,-summary}.json:72suites/6389cases/17ignored/2empty/zero logical duplicates. Fresh-output collision guards reviewed. Later profile parity compares suites including ignored/filter facts. B3 read-only isolation assessment ongoing; retain native/global-state targets. B4 decisions await symbols: one jobs4 paired trial if pressure persists; alternative linker only material residual cost. sccache unavailable, no demonstrated expensive dependency requiring feature pruning, broad crate extraction deferred.
