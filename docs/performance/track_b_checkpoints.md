# Track B checkpoint ledger

Baseline: cargo-build-perf at 6a6be519685652a282f7d0a411008601f158d075.
Branch: build-optimization. No push/merge.

| Checkpoint | State | Evidence / findings | Reviewer | Local commit SHA |
| --- | --- | --- | --- | --- |
| B0-A | PASS | Clean initial HEAD; metadata and host inventory; actual MSVC14.44 linker verified | planner/parent | dfc325f0 |
| B0-B | PASS | Harness self-tests; exit 7, arguments, env/cwd, timing freshness; null target override fixed | approved; null target inference fixed | 2849632c |
| B0-C | PASS | Frozen launcher244.683s/all-bin323.159s; branch243.453/240.955s; broad943.249s;6389cases/17ignored; PE original verified | approved; attribution/roster collision findings resolved | 3b6e4af8 |
| B1-A | PASS | Eight invalidation checks; alternate/original canonical release PE payloads verified; fixtures clean | source/generator approved | this checkpoint commit |
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

B0 frozen at3b6e4af8; production baseline preserved. Original batch had23successfulmeasurements and a final clean-tree assertion failure for generated clipboard_modifiers.json; preserved/moved only owned data, clean restoration verified. Branch queue86511 completed0, both directions measured twice; original PE verified. No Cargo job remains active.

B1-A source change: exactly2resource rerun directives, unchanged embed_resource::compile options/link breadth. Benchmark-only candidate615bb025b220972d170b7521422dcf098c93e1bb fromB0freeze; active normalized production content matches candidate. Eight check probes passed; source review approved. Initial probe49199 exited1 before icon builds because SystemIcons.Save emitted malformed generated ICO directory metadata; strictvalidator rejected, allsource restored. Failed evidence preserved. Generator correction affects only freshfixture reserved/planes/bpp from validatedDIB; reviewer approved. Guarded resume35390 verified prior summaries/manifests/sourceSHA/cleanstate, then alternate release247.335619s and original restore244.137434s both passed, PEpayloads verified, allfixturehashes restored. Raw evidence b1-resource-probes-1 and b1-resource-probes-icon-resume-1 under target/track-b.

B1-B next: optional four-preset Cargo wrapper plus accurate README/devREADME direct commands, fixedtarget/profile selectors and argument array forwarding; preserve all-bin command and auxiliary binaries. Metadata/combined bin check and wrapper help/exit forwarding checks once, reused by B1-C. B1-C transient run-b1-matched.ps1 parserpassed, NOTRUN, requires acceptedB1SHA; source transition/noop3/small2/originalrestoration preparation separate.

B2 support runner run-b2-symbol-pairs.ps1 is prepared but NOTRUN and undergoing static corrections: lib-map Values, original artifact preparation before later edited samples, fixturehash variable, automatic Args variable collision. No profile changes. Native startup helper statically approved, NOTRUN. Canonical release/default full-debug unchanged; lto=false differs from off. No default-run/editor/runtime changes.

Normalized baseline roster baseline-nextest-roster-final{,-summary}.json:72suites/6389cases/17ignored/2empty/zero logical duplicates. Helper CreateNew guards reviewed. Later profile parity compares suites including ignored/filter facts; grouping requires explicit migration map. B4 decisions pending after symbols: onejobs4 paired trial if pressure persists, alternative linker only material residual cost; sccache unavailable, feature pruning no expensive dependency evidence, crate extraction normally deferred.
