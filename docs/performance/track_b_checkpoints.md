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
| B2-B | PASS | Candidate5819774e; all20commands/restores/clean; small239.982265→27.062325s; both original PE/native passed | final four-file gate approved | d72c2703 |
| B2-C | PASS | Retain two measured optional profiles; no extra grid/cache; full-debug/default release unchanged | selection approved | 6620448d |
| B2-D | PASS | Document commands/cache paths/env override boundary; reuse actual flags/full-debug/assertions/original PE/native | final four-file docs approved | 58699bed |
| B3-A | PASS | Live78targets/67integrations; baseline72suites/6389cases/17ignored; bounded isolation audit/unknown-retain | source audit and report approved | ba2dc7b3 |
| B3-B | PASS | Target-first recipes; all6warm commands no compile; unrestricted3.004230s/lib1.873911s medians; scoped passes | final docs/empirical review approved | 392d9f90 |
| B3-C | SKIPPED | No audited pilot with demonstrated payoff; all67integrationtargets/test sources retained | source classification approved | 392d9f90 |
| B3-D | PASS | Runner51101 exit0; strict72suites/6389cases/17ignored/2empty parity and108scoped passes; scratch clean | final empirical/docs review approved | b5b594a3 |
| B4-A | PASS | Application library codegen dominates release edits; broad fanout separate; jobs4 screen justified, linker gate deferred | planner/final reviewer approved | 75eaa8d7 |
| B4-B | SKIPPED | Canonical main compile/link upper bound~7%; actual link cost unisolated; future test-linking hypothesis retained | gate/final docs approved | 4221c349 |
| B4-C | SKIPPED | sccache unavailable; no installation/wrapper/hit claims | planner/final docs review approved | 47cef167 |
| B4-D | PASS | Screen/reverse both exit0, exact7suite5368case17ignored parity; replay default61.057936/jobs463.220442s; retain default | screen/reverse/final docs approved | 4221c349 |
| B4-E | SKIPPED | No expensive warm dependency rebuild/removable feature established; graph unchanged | planner/final docs review approved | 47cef167 |
| B4-F | PASS; analysis complete, extraction DEFERRED | Shared invalidation proven, no stable costed extraction boundary; no source migration | architecture/final docs review approved | 47cef167 |
| B5-A | PASS | Frozen F4221c349; audited inputs/coverage/helpers; final active canonical build1.042136s Fresh; original PE/native passed | final empirical/documentation review approved | d31b17a0 |
| B5-B | PASS | All-bin316.836231s; branch234.581312/232.882807s; iteration small27.062325/large31.349379s; phases/cache/limits explicit | final methodology review approved; table finding fixed | this checkpoint commit |
| B5-C | pending |  |  | — |

## Execution state

B0–B4 are complete; the table records their local commits and conditional decisions. Historical measurements, failed fixture/parser preparations, remedies, cache phases and reviewer findings remain in track_b_baseline.md and track_b_results.md. No primary cache deletion, global tool/editor change or remote Git operation occurred.

Final production inputs are frozen at 4221c349acc34beb07d57852b70bfddbed0931ad (F). Compared with initial 6a6be519685652a282f7d0a411008601f158d075, production changes are exactly two build.rs resource watches and two opt-in Cargo profile tables. Runtime sources, tests, Resources, dependencies, lockfile and Cargo configuration are unchanged. Independent input/coverage/helper reuse audits retain original measured SHAs and exact fixture hashes.

B5 gaps runner19238 terminated0 at2026-10-10T18:12:17UTC. target/track-b/b5-gaps-1/success.json records16successful Cargo commands,6original preparations,2all-bin edited samples,1second large sample per profile,2branch preparations,4branch transitions and4fixture restorations. Scratch is clean at F; original color/GUI hashes restored; create-only owned A/B refs retained. Observer1283 terminated0;55sparse60second samples are observations, not true peaks. Independent final gap/restoration/resource-summary review approved.

Final check runner80125 terminated0: original preparation36.228948s excluded; three no-compile warm checks1.378059/0.946584/0.882092s. Final active-checkout canonical release command passed1.042136s Fresh at F; corrected evidence is outside the source worktree, as required by the existing helper. Original icon/PE comparison and bounded owned native startup/normal close passed with matching executableSHA9d78d3893fea30addc0221685d8675e2b52144319eb28be7faf1e50d8b3fc5e5. No Cargo or owned smoke remains active.

B5-A is committed at d31b17a0818f77b4fcb720149d634387287908c7. B5-B final comparison review approved; its local commit and B5-C local handoff are being closed. Full-suite execution, ignored-case execution, non-Windows checks, manual debugger inspection, exact link duration, true peak RAM, runtime-performance equivalence and iteration branch-switch benchmarking were not performed; these limits do not imply missing conditional gates. The closest measured six-minute workload remains default all-bin edited release; the original user command is unknown.
