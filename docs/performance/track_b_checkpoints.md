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
| B1-C | PASS | no-op1.015507s; edited258.432568s (+5.62%, cause unresolved); all8commands0/clean restored | approved; no speedup claim | 50e3104a |
| B2-A | PASS | fast-dev first/replay launcher34.75%/29.69% faster; tests10.76%/9.23%;32history observations/2restored color passes; icon/native passed | approved; phase/cache caveats | pending local commit |
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

B0 frozen at3b6e4af8: all23measurement commands passed; generated clipboard_modifiers.json was preserved/moved only after ownership/hash verification, then clean restoration verified. Replacement branch queue86511 completed0. Original PE icon verified; no cache deletion. Initial branch/SHA remains cargo-build-perf/6a6be519685652a282f7d0a411008601f158d075.

B1-A ba3f7f59: exactly two resource watches; embed_resource options/link breadth unchanged. Eight check probes passed. Initial malformed generated ICO fixture rejected before icon builds, source restored. Reviewer-approved new-fixture correction and guarded resume35390 passed alternate/original canonical release PE comparisons247.335619/244.137434s. Raw b1-resource-probes-1 and b1-resource-probes-icon-resume-1 retained.

B1-B73fce728: wrapper/docs approved; metadata78targets/fourbins; combined four-bin check28.667616s; argv/help/exit/cwd/env checks passed. B1-C50e3104a: runner94284 terminal0/all8commands passed/source restored. No-op1.015507s; edited258.432568s, observed5.62% slower versus B0, cause unresolved/no speedup claim. Same release flags verified. Baseline native14274 terminal0: expected window/WM_NULL/oneclose/normalexit0/ownedcleanupclear; b1-release-native-1. Isolated startup, not final B5 proof.

B2-A benchmark candidate7b6fab5e994be80875d18a09b0bc9ee80cc0b376 from50e3104a matches active production manifest. fast-dev inherits dev, changes only debug to line-tables-only; default dev/test/release unchanged. Initial94076 terminal1 on duplicate identical FAIL parser; all31commands retained and source restored. Corrected exact-identity parser/guarded resume approved after PowerShell containment fix. Resume56115 TERMINAL0 at15:14:30UTC, evidence b2-symbol-pairs-resume-1:18noops/four expectedfails/four8-test historyruns/two restoredcolorpasses/fourrestores/fourlibbinaries; original407d4a9...ee9c and scratch clean verified. Read-only aggregation suffix collision corrected; report helper PASS, no builds repeated.

B2-A first-edit/replay comparisons are separate cache phases, not interchangeable repetitions: launcher dev144.058486/43.251080 versus fast93.992278/30.412154s; library-test preparation dev196.956480/40.201463 versus fast175.760159/36.489082s. Check mixed; all no-op medians slower. Existing dev cache/new fast cache and resume gap disclosed. Population excluded. Recommend opt-in launcher/Nextest, retain default check. Independent source/results review approved. Debugger variable/breakpoint inspection NOT RUN; safety inherited by configuration, actual assertion/backtrace gates passed.

B2-A original fast-dev launcher restoration57563 terminal0/104.561176s excluded. PE Verifiedtrue and native19996 terminal0/PASS (b2-native-preparation-1 and b2-fast-dev-native-1): expectedwindow/WM_NULL/oneclose/normalexit0/rootgone/cleanupclear, binarye729ea720a877e96290190cf2ec2281df421c7fff577af34ed98fa85ae8e90b3. Native source7b6fab5e; no active Cargo/application. B2-A local commit pending final documentation review/diff.

B2-B ignored iteration runner20step schedule independently approved; no profile installed/executed. Candidate inherits release with opt2/64CGU/lto="off"/incrementaltrue. lto="off" differs from default false; report combined tradeoffs. First population separate; postpopulation noops establish freshness. Execute only after B2-A local commit, one shared target writer. No parameter grid absent evidence. B2-C/D will select measured configuration and verify unchanged canonical flags/full-debug escape.

Normalized frozen baseline roster baseline-nextest-roster-final{,-summary}.json:72suites/6389cases/17ignored/2empty/zero logical duplicates; output collision guards reviewed. B3 read-only classification approved: retain all67integrationtargets, unknown-retain uninspected targets; conditional grouping SKIP recommended without material measured payoff. Settings::default reads cwd; manager target includes shared runtime/native construction, so six parser cases do not make all8 pure. Nextest per-test processes alone do not establish grouping danger. Full profile discovery/parity and targeted history/domain/native execution pending.

B4-A prepared ranking: B1 library mean238.905s (~92.4% editwall), frontend69.280/codegen169.625s; main compile/link18.540s (~7.2%, not exact linkduration). Broad discovery943.249s is separate. Matched logs only app dirty/compiling, dependencies Fresh. B4 linker/jobs decisions remain gated on residual post-symbol costs; possible one jobs4 trial, no broad sweep. sccache unavailable; feature pruning lacks demonstrated expensive dependency; broad crate extraction deferred.

B5 gap packet prepared: reuse prior finalized measurements only after exact production-input/effective-flag equivalence audit, keeping actual measured SHA. Still requires final large-edit second sample, default-all-bin small-edit two samples, final controlled branch pair two directions/two each, full roster parity/appropriate scoped executions. No full suite execution or cold campaign. Finish all scratch measurements before final active-checkout canonical build/PE/native. Label scratch-to-active source-path transition separately; first active build may recompile application despite shared dependency cache. Leave production executable from accepted original active source, never benchmark mutation.
