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
| B2-A | PASS | fast-dev first/replay launcher34.75%/29.69% faster; tests10.76%/9.23%;32history observations/2restored color passes; icon/native passed | approved; phase/cache caveats | ac21724a |
| B2-B | PASS; local commit below | Candidate5819774e; all20commands/restores/clean; small239.982265→27.062325s; both original PE/native passed | final four-file gate approved | — |
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

B2-A original fast-dev launcher restoration57563 terminal0/104.561176s excluded. PE Verifiedtrue and native19996 terminal0/PASS (b2-native-preparation-1 and b2-fast-dev-native-1): expectedwindow/WM_NULL/oneclose/normalexit0/rootgone/cleanupclear, binarye729ea720a877e96290190cf2ec2281df421c7fff577af34ed98fa85ae8e90b3. Native source7b6fab5e; B2-A Cargo/application jobs terminated. B2-A committed locally ac21724a281f784f240de51c27eeef5a28ea13a6 after final documentation corrections/review; clean branch verified.

B2-B benchmark-only candidate5819774e28491fead6d4634488969f60dc0115d7 fromac21724a; runner19621 TERMINAL0 at16:03:44UTC, evidence b2-iteration-1. All20commands passed:2populations/6noops/4small/2large/6originalpreparations;6restorations, exact original color/GUI hashes and scratch clean. iteration inherits release opt2/64CGU/lto="off"/incrementaltrue; canonical unchanged. Small first/replay release235.617816/244.346714 versusiteration26.147305/27.977344s. Large n1 release238.326693/iteration28.489376. No-op medians0.885758/1.389327s, iteration slower. Unequal-history populations/preparations excluded. Exact link/peak NOT MEASURED. Combined tradeoffs, not separate parameter causation.

B2-B final original release/iteration preparations256.312369/29.314498s excluded. Both PE Verifiedtrue and native runner42487 TERMINAL0/PASS (b2-release-native-1/b2-iteration-native-1): expectedwindow/WM_NULL/oneclose/normalapp+helperexit0/rootgone/cleanupclear/notimeout. BinarySHA release0d6483b1df0bb9dd7ae3d92cabfe6483230b30af66235661e2e5965b4edb8ab3; iteratione7e78b4d4322e109568a39364839e802583ed87248aca2c72a81a4e3c06f076b. PDB larger15,069,184→21,491,712bytes; iteration cache3,988,730,009logicalbytes/3,737files, unequal cache histories. Source/results/native independent review approved; acceptance docs/local commit pending. B2-C/D planner packet ready, no extra profiles/grid or iteration test-cache recommendation.

Normalized frozen baseline roster baseline-nextest-roster-final{,-summary}.json:72suites/6389cases/17ignored/2empty/zero logical duplicates; output collision guards reviewed. B3 read-only classification approved: retain all67integrationtargets, unknown-retain uninspected targets; conditional grouping SKIP recommended without material measured payoff. Settings::default reads cwd; manager target includes shared runtime/native construction, so six parser cases do not make all8 pure. Nextest per-test processes alone do not establish grouping danger. Full profile discovery/parity and targeted history/domain/native execution pending.

B4-A prepared ranking: B1 library mean238.905s (~92.4% editwall), frontend69.280/codegen169.625s; main compile/link18.540s (~7.2%, not exact linkduration). Broad discovery943.249s is separate. Matched logs only app dirty/compiling, dependencies Fresh. B4 linker/jobs decisions remain gated on residual post-symbol costs; possible one jobs4 trial, no broad sweep. sccache unavailable; feature pruning lacks demonstrated expensive dependency; broad crate extraction deferred.

B5 gap packet prepared: reuse prior finalized measurements only after exact production-input/effective-flag equivalence audit, keeping actual measured SHA. Still requires final large-edit second sample, default-all-bin small-edit two samples, final controlled branch pair two directions/two each, full roster parity/appropriate scoped executions. No full suite execution or cold campaign. Finish all scratch measurements before final active-checkout canonical build/PE/native. Label scratch-to-active source-path transition separately; first active build may recompile application despite shared dependency cache. Leave production executable from accepted original active source, never benchmark mutation.

B2-D supporting evidence extracted from both completed population logs: population-application-profile-flags.json records lib/bin canonical opt-level3/embed-bitcode=no/strip=debuginfo and iteration opt-level2/lto=off/codegen-units64/incremental. Only explicit flags are asserted; defaults and safety are not separately executed tests. B2-D completion still requires final accepted configuration/original artifacts and documentation gates.

B3-D installed Nextest list help verified --cargo-profile, --message-format, --run-ignored, --ignore-default-filter and --list-type(full default), without Cargo compilation. Upcoming explicit reachability commands are listing only, not ignored-test execution.

B3 ignored run-b3-parity.ps1 prepared by parent; parser PASS after reserved Clean keyword collision fixed. Independent read-only review approved after cleanup-finally/CARGO_BUILD_TARGET guards fixed; static topology/roster identity checks PASS; NO Cargo/helper execution. It implements strict baseline suites parity plus explicit full ignored-case reachability, selected108test executions, preserved known generated fixture and warm target-selection overhead comparisons; B2-A8history cases reused as prior evidence. B5 gap runner is separately materialized by implementer and undergoing static checks, not executed.
