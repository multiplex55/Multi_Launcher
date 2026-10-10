# Track B results

Baseline: cargo-build-perf at `6a6be519685652a282f7d0a411008601f158d075`. Implementation: build-optimization; local commits only, no push/merge.

B0 is frozen. B1–B4 are complete; validated B5 values are filling the table below. Branch transitions, warm checks and the final active-checkout native gate remain pending. Full protocol and cache caveats are in track_b_baseline.md.

| Scenario | Baseline seconds | Final | Comparison constraint |
| --- | --- | --- | --- |
| No-op release launcher | median1.017300; range0.997221–1.058698; n=3 | median0.885758; range0.868423–0.969772; n=3 | Reused5819774e/equivalentF; small absolute difference, no strong gain claim |
| Small edited release launcher | median244.682906; range239.562765–249.803047; n=2 | median239.982265; range235.617816–244.346714; n=2 | Same color fixture; observed1.92%lower/ranges overlap; no substantial canonical gain |
| Large edited release launcher | median240.309998; range237.433298–243.186698; n=2 | median236.646632; range234.966570–238.326693; n=2 | Same GUI fixture; validated5819774e first/F second; ranges overlap |
| Default all-bin edited release | median323.159398; range317.662238–328.656558; n=2 | median316.836231; range315.332734–318.339727; n=2 | Observed1.96%lower; ranges overlap; final restoration gate pending |
| Branch-switch release A→B / B→A | medians243.453353 / 240.954734; n=2 each | NOT MEASURED | Owned refs, exact same color change |
| Library Nextest preparation / cached invocation | 210.186021 / 2.009562 | See separate B5-B phase table | Preparation, no-run and eight-test execution are distinct |
| History preparation / cached invocation | 215.092202 / 2.077256 | See separate B5-B phase table | Five passed; normal Cargo prerequisites and differing histories |
| Domain preparation / cached invocation | 20.862317 / 2.796195 | See separate B5-B phase table | 102 passed; prior prerequisites differ |
| Broad Nextest compile/discovery | 943.248524;6389cases | default938.288174/fast-dev265.721831; n=1each | Unequal cache histories; no isolated speedup; inventory not full execution |
| Check target population / warm checks | 76.682837 / 1.078176,0.918521 | NOT MEASURED | Keep first population separate |
| Named iteration profile | No prior profile; matched release controls below | Small median27.062325; large31.349379; no-op1.389327 | Opt-in combined-profile comparison; population/preparations separate; no canonical equivalence claim |

The all-bin median was78.476492s slower under the same edit; extra targets are the intended workload difference. Sequential trials and differing library durations prevent attributing every second solely to target selection. The all-bin baseline is closest to the approximate six-minute report; original user command is not established. Named-profile results below do not represent canonical release improvements.

Conditional B4 decisions are final: retain default jobs after reverse confirmation showed no repeatable jobs4 benefit; retain MSVC because canonical linker cost is not sufficiently established to justify a trial. sccache unavailable; dependency-feature gate unmet; crate extraction deferred after analysis. No global tooling, dependency or workspace change.

B0 PE/icon verification passed against original ICO after branch restoration. Inventory:72suites,6389cases,17ignored,2empty; zero duplicate logical cases. Native startup was NOT RUN at B0; the later B1 smoke is recorded below. Independent baseline/support review approved after causal-attribution wording and fresh-output collision fixes.

## B1-A resource invalidation (PASS)

Candidate `615bb025b220972d170b7521422dcf098c93e1bb` is a benchmark-only commit from B0 freeze, containing only the two resource rerun directives. All probe gates passed before the implementation commit. The original embed_resource call/options remain unchanged; source review approved.

| Check probe | Seconds | Script run | Script compile |
| --- | --- | --- | --- |
| Initial candidate | 16.794472 | yes | yes |
| Identical unchanged | 1.016461 | no | no |
| Exact B0 color edit | 13.582826 | no | no |
| Color restore preparation | 12.852385 | no | no |
| RC comment edit | 13.809610 | yes | no |
| RC restore | 12.775486 | yes | no |
| build.rs comment edit | 12.372675 | yes | yes |
| build.rs restore | 12.662456 | yes | yes |

All eight commands exited0 and matched expected compile/run observations. These check timings establish invalidation behavior, not an edited release speedup. Raw evidence: target/track-b/b1-resource-probes-1.

The first probe batch stopped before icon builds because SystemIcons.Save generated malformed ICO directory fields (reserved167, planes/bpp0), despite a consistent DIB payload. Strict validation rejected it; all source hashes/clean restoration passed. Failed fixture/logs are retained. The corrected generator canonicalizes only a newly generated fixture directory (reserved0, planes/bpp from validated DIB), leaving canonical asset and strict validator unchanged; reviewer approved. Guarded icon-only resume validates all eight prior summaries/manifests, candidate SHA and clean source, then runs alternate/restored icon release builds and PE comparisons. Both gates passed in target/track-b/b1-resource-probes-icon-resume-1; exec35390 terminated0. Alternate release247.335619s / original restoration244.137434s, both exit0 with actual build-script rerun and byte-exact PE payload matches. All fixture hashes restored and scratchGit clean. No broad rerun or duplicate check campaign. Active/candidate production content comparison was empty across build.rs/Cargo.toml/Cargo.lock/src/Resources/tests.

## B1-B target workflows (PASS)

README and dev README now recommend explicit launcher build/run and target-selected tests, retain deliberate all-bin release builds, and explain shared-library compilation. The optional cargo.ps1 wrapper fixes repository CWD/manifest and uses native ArgumentList forwarding for release/debug/lib-test/integration-test presets. It rejects conflicting compiler/target/package selectors while permitting Nextest runner profiles; advanced workflows remain direct Cargo commands. No default-run, manifest, lockfile or runtime changes.

Accepted B1 source metadata still reports78targets and4binaries; combined explicit4-bin check passed28.667616s, exit0 (target/track-b/b1-bin-preflight). This scoped compile evidence is reused for B1-C rather than repeated for documentation changes.

Implementer checks passed: parser; four PrintCommand presets; spaced filters; arguments after --; Nextest runner profile pass-through; missing integration target and long/short/packed selector rejection; invocation from TEMP with caller cwd/environment unchanged; Cargo build/Nextest run help exit0; invalid-option native exit1 identical direct and through wrapper, without compilation. Parent corrected comment-help placement and verified synopsis, description and three examples. Independent final source review approved, no outstanding findings. Parent diff check passed.

## B1-C matched low-risk verification (PASS)

Accepted source `73fce728c91e47d2b7e122bf91aab7b5e72dfbac`; same owned scratch path, primary target, toolchain, exact B0 color edit and canonical `cargo build --release --bin multi_launcher --timings -vv`. Raw evidence: target/track-b/b1-matched-1. All eight commands exited0; runner94284 terminated0, success marker and byte-exact original color hash/clean scratch verified.

| Scenario | B0 seconds | B1 seconds | Interpretation |
| --- | --- | --- | --- |
| No-op, n=3 | median1.017300;0.997221–1.058698 | median1.015507;0.923352–1.069035 | Essentially unchanged |
| Same small edit, n=2 | median244.682906;239.562765–249.803047 | median258.432568;257.320716–259.544420 | Observed5.62% slower; no demonstrated release speedup |
| Original restoration preparation, n=2 | excluded |239.838158,244.770315 | Excluded from edited samples |
| Accepted-source transition | excluded |17.966304 | AllFresh/no compile observed; cause unassigned |

Both edited B1 builds kept the resource script fresh, whereas B0 reran it. Cargo timing aggregates: library236.84/240.97s, frontend73.70/64.86s, codegen163.14/176.11s; launcher19.53/17.55s. Exact linker duration remains NOT MEASURED. The observed slowdown's cause is unresolved; neither causal regression nor random variability is established. No performance claim is made for the two resource watches. Independent review verified unchanged opt-level3/embed-bitcode=no/strip=debuginfo, metadata/link arguments, fixture hash, command and production content outside the two watches; no correctness findings. Four-bin check/metadata from B1-B and icon/resource gates from B1-A are reused. The final matched comparison is recorded in B5-B below.

The restored B1 production binary passed the bounded native startup smoke (target/track-b/b1-release-native-1, runner14274 exit0): expected visible launcher window, responsive WM_NULL, one posted WM_CLOSE, normal application exit0, root absent and all recorded owned cleanup clear. Source73fce728; executableSHA4164f1856d9967d074e7ebcfd53f6e96c8e27165d389c106a29d713a9448a70b. The14.790s total includes helper/polling/cleanup and is not application startup latency. This isolated startup gate does not establish full plugin/hotkey/render equivalence or final-source B5 validation.

## B2-A reduced development symbols (PASS)

Accepted opt-in profile: `[profile.fast-dev]`, `inherits = "dev"`, `debug = "line-tables-only"`. Default dev/test and canonical release are unchanged. Actual rustc commands retain incremental compilation and change debuginfo2 to line-tables-only. Inheritance preserves development debug assertions and overflow checks by configuration; no separate overflow or debugger-variable inspection was executed. Full debugging remains available through default Cargo commands.

Benchmark-only source `7b6fab5e994be80875d18a09b0bc9ee80cc0b376` contains exactly this production manifest change from B1-C. Same scratch source/primary target/toolchain and exact B0 color fixture. Raw evidence: target/track-b/b2-symbol-pairs-1 and b2-symbol-pairs-resume-1. Commands are launcher `cargo build --bin multi_launcher --timings -vv`, selected-library `cargo nextest run --lib --no-run`, and `cargo check --lib --timings -vv`, adding `--profile fast-dev` for build/check or `--cargo-profile fast-dev` for Nextest. Preparation, execution and first profile population are separate.

| Workload / cache phase | Default dev seconds | fast-dev seconds | Observed comparison |
| --- | --- | --- | --- |
| Launcher, first measured edit | 144.058486 | 93.992278 | 34.75% faster |
| Launcher, replay of exact edit | 43.251080 | 30.412154 | 29.69% faster |
| Library test preparation, first edit | 196.956480 | 175.760159 | 10.76% faster |
| Library test preparation, replay | 40.201463 | 36.489082 | 9.23% faster |
| Library check, first edit | 37.153498 | 17.654693 | 52.48% faster |
| Library check, replay | 12.629091 | 13.493055 | 6.84% slower |

These are two distinct cache phases, not interchangeable repeated trials of arbitrary edits. Each replay follows an excluded original-source artifact rebuild. Default dev had an existing cache; fast-dev was a new profile. Retained incremental work plausibly contributes to the large first/replay difference, but exact causation was not measured. The controlled sequence and parser-resume gap also limit generalization. Both launcher and selected-test preparations improved in both observed phases; check results are mixed. Recommend fast-dev for opt-in launcher/Nextest work, retain default `cargo check --lib`, and make no universal speedup claim. Descriptive two-value medians/ranges: launcher dev93.654783 [43.251080,144.058486], fast62.202216 [30.412154,93.992278]; test preparation dev118.578972 [40.201463,196.956480], fast106.124620 [36.489082,175.760159]. Phase rows are the primary comparison.

No-op medians (n=3 each), all without project compilation: launcher dev0.880518/fast1.350037; library no-run dev1.640601/fast1.719634; check dev0.865623/fast1.345326. All fast-dev no-op medians are slower. Initial population is excluded: dev launcher224.932473/test53.130679/check17.629025/list1.784728; fast launcher224.459127/test175.449068/check107.159440/list1.798085 seconds. Dev launcher compiled the application with340dependencies Fresh; fast launcher compiled341units, including340dependencies. Thus similar population wall times are not a fair matched speed comparison. Separate target/fast-dev cache adds storage. Read-only accumulated-cache scan at b2-native-preparation-1/cache-footprint.json measured fast-dev10,796,459,402logical bytes/7,361files and existing debug707,885,473,647bytes/137,453files. Histories differ substantially: these are current logical totals, not a normalized profile-size ratio or physical disk allocation.

Immediate edited launcher EXE sizes: dev90,557,440bytes/fast81,320,448bytes; PDB dev593,547,264 and593,612,800bytes/fast256,151,552bytes (about57% smaller). Final original-source library-test EXE dev129,101,824/fast117,190,656bytes; corresponding PDB821,506,048/329,224,192bytes (about60% smaller). Initial library PDB sizes were not captured; do not infer them from final artifacts. Exact linker duration and peak memory NOT MEASURED. Two sparse resource snapshots cover candidate test population and original-artifact preparation, with available memory19,550/20,402MB and zero sampled page-input/read rates; these are not sustained or matched-edit resource comparisons.

All four sets of eight history tests passed (32pass observations); each deliberate color mutation produced the exact expected RGB assertion and src/color.rs:93 application frame, native exit100. Both final original-source color tests passed. Success marker verifies18noops, four expected failures, four fixture restorations, four resolved library binaries and identical original/final color hash407d4a9...ee9c; scratch clean. The initial runner stopped after31commands because identical Nextest FAIL lines were counted twice. The reviewed correction deduplicates exact failure identities, validates all31retained commands and resumes in fresh evidence at032 without remeasurement. A reviewer also corrected the resume directory-containment expression. A separate read-only reporting suffix collision was corrected to anchored scenario identities. All failed evidence is retained; none represents unexpected application test failure.

Original fast-dev launcher restoration104.561176s is excluded preparation, followed by byte-exact original ICO/PE verification and bounded native smoke (b2-native-preparation-1 and b2-fast-dev-native-1; both exit0). ExecutableSHAe729ea720a877e96290190cf2ec2281df421c7fff577af34ed98fa85ae8e90b3: expected visible launcher window, responsive WM_NULL, one WM_CLOSE, normal exit0/root absent/all owned cleanup clear. Total13.519s includes helper/polling/cleanup, not startup latency. This verifies isolated startup, not full plugin/hotkey/render or runtime-performance equivalence. Independent review recommends this bounded opt-in profile; no substantive source findings remain. Full roster/profile parity and final B5 gates remain pending.

## B2-B release-like iteration profile (PASS)

Candidate benchmark source5819774e28491fead6d4634488969f60dc0115d7 from accepted B2-Aac21724a. The sole additional production configuration is iteration inheriting release with opt-level2/codegen-units64/lto="off"/incrementaltrue. This is a combined optimization/codegen/LTO/incremental tradeoff, not distribution-equivalent to canonical release and not a test-profile recommendation. Git comparison against B2-A measured7b6fab5e confirms unchanged Rust/resource/test/lock/config inputs. Effective application compiler flags were captured in b2-iteration-1/population-application-profile-flags.json: canonical lib/bin retain opt-level3/embed-bitcode=no/strip=debuginfo; iteration explicitly selects opt2/ltooff/64CGU/incremental. Absent options retain defaults; no independent safety/debugger test is implied.

| Completed small-edit sample | Canonical release seconds | Iteration seconds |
| --- | --- | --- |
| First measured color edit | 235.617816 | 26.147305 |
| Replay after original artifact preparation | 244.346714 | 27.977344 |

Same source path, primary target, exact B0 color fixture/toolchain and selected launcher. Both samples compiled and exited0; original preparations are excluded. The two phase values have descriptive medians239.982265s release and27.062325s iteration (88.72% lower), with ranges235.617816–244.346714 and26.147305–27.977344. This describes this exact fixture, not arbitrary-edit latency. Large-GUI screening/control was238.326693s release versus28.489376s iteration (n=1 each); a second final large sample remains B5 work.

Initial preparation/population was release295.568689s and iteration359.930962s, with different dependency/cache histories, not a fair cold speed comparison. Three no-ops per profile had medians0.885758s release and1.389327s iteration; ranges0.868423–0.969772 and1.373563–1.433014. All six showed no compilation. Six original-artifact preparations are excluded, including final release256.312369s and iteration29.314498s. Runner19621 terminated0 at16:03:44UTC: all20commands passed, six fixtures restored, original color407d4a9...ee9c/GUIb2379f2...5975 hashes identical and scratch clean. Raw evidence b2-iteration-1 retains every command/manifest/timing report.

Original release EXE48,880,128bytes/PDB15,069,184bytes; iteration EXE47,192,064bytes/PDB21,491,712bytes. Iteration PDB is larger, not a symbol-size gain. Read-only cache scan measured3,988,730,009logical bytes/3,737files after this sequence; physical allocation NOT MEASURED and older caches have different histories. A sparse population snapshot recorded8rustc processes, summed working set1,513,279,488bytes,20,954MB available,54page-inputs/s and19page-reads/s, G:queue0/write33,649,968bytes/s. This is neither peak memory nor sustained/matched-edit evidence. Exact linker duration and peak memory remain NOT MEASURED; profile gains cannot be assigned separately to optimization, CGU, LTO, incremental compilation or linking.

Both final original-source PE icon payloads verified byte-exact against the canonical ICO. Serial bounded native checks passed in b2-release-native-1 and b2-iteration-native-1 (runner42487 exit0): expected visible launcher, responsive WM_NULL, one close, normal helper/application exit0, root gone/all owned cleanup clear/no timeout. Source5819774e; binarySHA release0d6483b1df0bb9dd7ae3d92cabfe6483230b30af66235661e2e5965b4edb8ab3 and iteratione7e78b4d4322e109568a39364839e802583ed87248aca2c72a81a4e3c06f076b. These are isolated startup/resource checks, not full plugin/hotkey/render or runtime-performance equivalence. Independent review found no substantive defect and accepted this bounded opt-in after restoration/native gates. Default dev/test and canonical release remain unchanged; no iteration Nextest recommendation or production-runtime claim is made.

## B2-C selected codegen/incremental tradeoff (PASS)

Keep exactly the measured fast-dev and iteration profiles; B2-B is committed locally at d72c2703e2530aecbfd780126710ae8b370ca6e7. No further manifest change or candidate grid is justified by the completed first/replay small-edit and large-edit screening results. The full-debug dev/test path remains available. iteration is for launcher builds; fast-dev is the measured selected-test option. No iteration test-cache population or unsupported Nextest recipe is introduced.

B0/B1 show the warm release bottleneck inside the application library: B1 edited-library mean238.905s, frontend69.280s and codegen169.625s, versus launcher compile/link mean18.540s. Dependencies were Fresh; final linker duration was not isolated. B2's combined settings substantially reduce this controlled edit workload while keeping canonical release untouched. This does not identify a separately optimal CGU count, optimization level, incremental setting or LTO policy. Further tuning would create more caches and expensive population runs without a demonstrated remaining requirement. Additional16/32CGU, incremental-off and optimization-level candidates were NOT RUN, not rejected on invented measurements.

Reuse the actual B2-A/B measurements, resource checks and native gates with their original measured SHAs. No build is repeated solely for this selection decision. Sparse resource samples do not establish peak memory, sustained CPU saturation or optimal job count; those limits remain explicit. Profile selection follows the planner packet and independent source/results review; canonical release, dependency graph, lockfile, runtime sources and all tests are unchanged by this checkpoint.

## B2-D production and debugging separation (PASS)

The README decision table and dev workflow now distinguish quick check, fast-dev launcher/Nextest, iteration launcher, default full-debug dev/test and canonical release. Output directories remain target/debug, target/fast-dev, target/iteration and target/release. No wrapper expansion or redundant debugging profile is needed: the existing default dev/test path supplies full symbols, as actually built and tested in B2-A. Manual debugger variable/breakpoint inspection was NOT RUN.

Reuse B2-A's actual default/fast-dev builds, assertions/source-line failures and passing restored-source tests at measured7b6fab5e, and B2-B's separate original release/iteration artifacts, actual flags and passing PE/native checks at5819774e. Documentation checkpoint6620448d contains no additional production changes. The canonical compiler flags still match B0/B1; named profile output separation is observed, not a claim of binary identity or full runtime-performance equivalence. iteration inherits release safety-check defaults; dev/test remain appropriate when development assertions are required.

Environment overrides are an explicit boundary: Cargo permits CARGO_PROFILE_*, CARGO_INCREMENTAL, Rust flags and configuration to change effective settings. Neither manifest nor helper prevents intentional user overrides. Measurement guards reject conflicting overrides and retain actual invocations; documentation explains equivalent-environment reproduction rather than falsely promising immunity. No override experiment or redundant build is needed to establish this configuration boundary. Git production-input comparison, scoped documentation review and diff check substantiate evidence reuse; final active-checkout production build remains a separate B5 gate.

## B3-A test fanout and isolation audit (PASS)

Live metadata at measured5819774e passed0.082478s (b3-a-inventory-1), confirming78targets/67integrationtargets:65top-level auto-discovered files and existing domain/plugin_queries aggregators. Rust/test sources and target declarations are unchanged from frozen B0. The normalized baseline roster records72suites,6389logical cases,17ignored,2empty and zero logical duplicates (baseline-nextest-roster-final.json and summary). It preserves binary ID, qualified case name, kind, ignored status and filter match; metadata-only cache entries are not logical cases. B3-D will compare both accepted compiler profiles and explicitly list ignored cases; no non-Windows execution is claimed.

Broad B0 compile/discovery took943.248524s, with observed multi-minute link-process lifetimes and paging/HDD pressure. Exact link durations and per-binary peak memory were NOT MEASURED. The library/launcher warm edit workload and broad test fanout are separate bottlenecks. Existing domain102cases and plugin_queries131cases already amortize executable boundaries. All67integrationtargets are retained; the default classification of every target not explicitly audited below is unknown-retain, not an assertion of purity.

| Inspected target / area | Classification and decision |
| --- | --- |
| query_autocomplete (1 case) | Potential small pilot; Settings::default reads the cwd screenshot directory, so construction is not strictly pure. At most one executable removed; no measured payoff establishes a migration. Retain. |
| multi_manager_plugin (8 cases) | Six parser cases appear pure, but two construct PluginManager with MkMacroStore/shared runtime/native catalog services. Moving six still leaves the executable. Retain. |
| history (5 cases) | cwd/persistence/application construction; retain fixture boundary. |
| watchers (3 cases) | cwd/filesystem/GUI dependencies; retain. |
| clipboard_persistence (2 cases) | cwd/environment/native clipboard dependencies; retain. |
| windows_plugin (1 case) | Native catalog enumeration/refresh; preserve Windows reachability. |
| window_manager (2 cases) | Shared MOCK_MOUSE_LOCK/position overrides with resets; mocks are not proof of native input behavior. Retain. |
| plugin_queries::help_plugin::search_returns_help_action | Pure case already grouped; no additional executable avoidance. |

Nextest isolates individual cases in processes: environment mutation alone does not prove grouping is unsafe. Conventional Cargo test concurrency, shared fixture ownership and native lifecycle still require a concrete audit before migration. The scoped planner/reviewer audit found no pilot with sufficient demonstrated benefit. No test code, identifiers, cfg/ignored attributes or fixtures are changed; unknowns retain their existing boundaries. Full roster parity remains the explicit B3-D gate, rather than assuming source module counts equal tests.

## B3-B targeted workflows (PASS)

Accepted recipes select Cargo targets before case filters and reuse the existing optional cargo.ps1 helper, without creating another runner interface. The default Windows roster selects8runnable library history_prepare cases (one additional matching benchmark remains normally ignored),5history,4domain indexer,1plugin_queries help and8multi_manager_plugin cases. Counts describe selection, not an assertion that every recipe was executed in this checkpoint. Manager execution was NOT RUN. Prior B2-A's four8-case history executions and current history5/domain102/windows_plugin1 passes supply scoped behavioral evidence, with actual measured sources retained.

Reviewed serial runner51101 terminated0 at16:45:47UTC on measured5819774e, same scratch/primary target, raw b3-parity-1; final scratch clean. Three alternating warm unrestricted case-filter-only no-run commands took3.004230/2.925251/3.172670s (median3.004230), versus explicit --lib1.873911/1.743275/1.890874s (median1.873911). All six had no compilation. This measures orchestration overhead, not compiler savings. The unrestricted command retains broad Cargo target selection even though its case filter matches library history cases; --lib constrains the requested target. Integration selection may still build normal binary prerequisites.

Separate original-source target preparations were default lib70.009479/history226.245197/domain22.483401s and fast-dev lib167.773999/history180.596997/domain8.785424s. Cache histories and preceding prerequisites differ, so these are preparation costs, not fair isolated profile speed comparisons. Broad default/fast compilation/listing took938.288174/265.721831s (n=1 each), treated separately from narrow execution and warm commands. Full discovery parity is recorded in B3-D. No full suite or manager execution is implied, and no test code was changed to obtain the workflow benefit. Recipe/source review approved target boundaries, nonzero selections and honest execution limits.

## B3-C conditional grouping pilot (SKIPPED)

The B3-A audit did not establish a small independent set with demonstrated executable-avoidance payoff. The query_autocomplete candidate would avoid at most one executable and still reads cwd-dependent settings. The six apparently pure manager parser cases would leave that target's two shared/runtime/native cases and its executable in place. Existing pure help/domain cases are already grouped. Broad fanout alone does not justify moving unknown/native/global-state tests across established fixture boundaries.

Retain all67integrationtargets and every source case/name/cfg/ignored attribute. No pilot, compatibility adapter, test movement or grouping benchmark was performed; this is a gated skip, not a failed migration or measured rejection. The scoped source audit and independent review support this decision. B3-D supplies the completed default/fast-dev discovery parity below.

## B3-D full discovery and target-layout parity (PASS)

Serial runner51101 completed all19commands with exit0 at2026-10-10T16:45:47UTC, measured source5819774e28491fead6d4634488969f60dc0115d7. Evidence: target/track-b/b3-parity-1, four normalized default/fast matched/full rosters, baseline-nextest-roster-final.json and per-command manifests/logs. Same installed Nextest/toolchain/features, source path and primary target as prior measurements. Both normally filtered rosters strictly match the frozen baseline:72suites,6389cases,17ignored,2empty,6372matched/17unmatched, zero logical duplicates. Metadata-only executable cache records are excluded from case identities.

Full listings add --run-ignored all --ignore-default-filter --list-type full --message-format json. Both profiles expose6389matched/0unmatched cases. Comparing binary ID, qualified case name, kind and ignored status gives exact identity parity with baseline; only the intentionally changed filter-match facts are excluded from this second comparison. Since no grouping occurred, the old-to-new map is identity for every case and all67integrationtarget names. Windows cfg/ignored attributes remain unchanged. Ignored tests were listed, not executed; non-Windows discovery/execution was NOT RUN.

| Separate phase | Default seconds | fast-dev seconds | Interpretation |
| --- | --- | --- | --- |
| Broad compile/discovery, n=1 each | 938.288174 | 265.721831 | Different cache/prerequisite histories; no isolated profile or grouping speedup claim |
| Already-built full reachability list | 3.155210 | 2.963865 | Listing overhead, not compiler work |
| Selected history execution | NOT RUN here | 3.326820;5passed | No compilation |
| Selected domain execution | NOT RUN here | 4.354443;102passed | No compilation |
| Selected windows_plugin execution | NOT RUN here | 2.166182;1passed | No compilation; legitimate empty-catalog branch remains possible |

The108scoped passes are separate from six target preparations and broad compilation. The known domain-generated clipboard_modifiers.json was hash-verified and preserved into owned evidence; final scratch source hashes and Git cleanliness passed. No full suite was executed. No target hiding, autotests=false, required-features, ignored-test edits, warning suppression or test-source changes were introduced. Native catalog testing does not prove particular windows were enumerated or exercise input hooks.

Sparse owned resource samples during default broad compilation observed8rustc/8link and8rustc/7link, G:queue7/10 and available15361/13537MB. A fast-dev broad sample observed7rustc/3link, summed compiler/link working set5,169,377,280bytes, available17987MB, system page-inputs5077.642/s and G:queue6. These are nonmatched snapshots, not peak memory, sustained pressure, exact linker time or proof of cause. They justify a bounded scheduling hypothesis for B4, not an accepted jobs setting. Independent empirical and documentation review approved preserved identities, execution limits, cache caveats and clean restoration.

## B4-A measured bottleneck ranking (PASS)

Reused read-only Cargo UNIT_DATA from b2-iteration-1/edited-application-unit-{timings,summary}.json and completed B3 evidence; no rebuild is needed for interpretation. For the two canonical small edits, library mean222.245s comprises frontend63.250/codegen158.995s, versus main compile/link16.735s. For iteration, library mean21.535s comprises frontend14.050/codegen7.485s, versus main3.940s. Application-library codegen dominates canonical warmed edits; frontend becomes the larger reported library phase under iteration. These unit aggregates overlap other work and do not isolate linker time.

Broad Nextest fanout remains a separate material workload: default938.288174s/fast-dev265.721831s, n=1 each with different cache histories. Concurrent rustc/link processes and sparse paging/HDD-queue samples justify testing scheduling. They do not prove linker causation or sustained memory exhaustion. Warm edited dependencies were Fresh; proc-macro/dependency rebuilds and unrelated resource reruns are not demonstrated causes of these edits. Named profiles retain measured no-op overhead, initial population and extra cache storage costs.

Actual Windows linker remains MSVC14.44.35207 HostX64/x64/link.exe; LLVM22.1.6 in rustc does not mean LLD is selected. Bundled rust-lld exists but has not been compatibility-tested. No global toolchain/configuration changes. Exact link duration and peak memory remain NOT MEASURED.

Decisions: run one bounded default-versus-build-jobs4 fast-dev screen on the fixed seven-suite5368-case cohort, confirm reverse order only if promising; no parameter sweep or automatic default change. Defer alternative-linker decision until that result because current residual evidence does not isolate sufficient linking cost. sccache unavailable: skip without installation or hit claims. Feature pruning gate unmet: no costly warm dependency rebuild with an identified removable feature. Crate extraction remains analysis-only/deferred: shared-library invalidation is real but no stable costed boundary or quantified additional avoidance is established. Independent planner/reviewer fact check and final documentation review approved this ranking.

## B4-C compiler cache availability (SKIPPED)

sccache was absent from the verified Windows host/tool search. No installation, PATH change, wrapper, private cache or cache-clearing operation was performed. Hit/miss/unsupported counts and branch-cache gains are NOT MEASURED. This availability skip is not evidence that sccache cannot help a later explicitly configured nonincremental workload; no incremental-compilation speedup is attributed to it. Existing Cargo caches and standard commands remain usable without an external cache dependency.

## B4-E dependency features (SKIPPED)

The actual warmed source-edit timing records keep dependency units Fresh and identify application-library frontend/codegen cost. No expensive rebuilt dependency with a plausibly removable feature was established. The conditional audit/pruning gate is unmet, so no cargo-tree campaign, feature experiment, package upgrade or Windows binding removal was performed. Cargo.lock and the entire dependency/feature graph remain unchanged, including the pinned rdev source. Required native/plugin functionality is not traded for speculative cold-build savings. Downstream monomorphization may still contribute to application codegen; that does not establish a safe unused feature.

## B4-F crate extraction decision (DEFER; analysis complete)

Both exact color and GUI fixtures invalidate the shared library and dependent selected binaries; broad test fanout is also measured. This establishes shared invalidation, not a costed extraction boundary. No inspected small stable domain boundary comes with quantified avoided recompilation, limited API/cycle/Windows coupling and a demonstrated payoff beyond the accepted iteration profile. A new crate would add API visibility decisions, cache/target population and test migration while preserving an expensive parent when edits still touch it. No credible numerical extraction saving is claimed.

Keep the present crate architecture. No proof of concept, workspace split or source/test migration was performed. A later extraction proposal should identify one pure owner, concrete callers, representative edits that avoid the parent rebuild and actual API/cache/maintenance costs before seeking separate implementation scope. The planner and independent architecture reviewer agreed that deferred extraction is appropriate for the evidence available here.
## B4-D build concurrency (PASS; jobs4 not retained)

One bounded fast-dev experiment selected --lib plus history/domain/plugin_queries/windows_plugin/multi_manager_plugin/window_manager, seven suites/5368cases/17ignored. Both jobs policies compile the same12application units, including four normal binary prerequisites. Build control uses Nextest --build-jobs 4; runner --jobs is not interchangeable. No global job setting or production source changed. Same scratch source5819774e, primary target, B0 color fixture and recorded environment.

| Exact edited workload phase | Cargo default seconds | Build jobs4 seconds | Decision constraint |
| --- | --- | --- | --- |
| Initial screen, default then jobs4 | 382.027895 | 64.533051 | First exact-fixture compilation versus later replay; not isolated scheduling gain |
| Reverse confirmation, jobs4 then default | 61.057936 | 63.220442 | Both replay the fixture; jobs4 supplies no repeatable advantage |

Keep Cargo default. Do not average the first default observation with its warmed replay and present an isolated jobs percentage. All preparations and roster checks are excluded; first screen original preps2.503323/52.542617/55.005868s, reverse preps2.343416/53.208790/70.156668s. The initial no-compile preparation differs from later artifact rebuilds and reinforces the cache-phase limitation. No sweep of6/8jobs, cache clean or configuration change was performed.

Actual verbose compiler arguments retain line-tables-only/incremental for all12units under both policies. Five separate normalized rosters per sequence must strictly match the frozen cohort, including ignored/filter facts; full campaign execution of these tests is not implied by no-run builds. Screen23350 passed all ten commands, five roster checks, two exact fixture restorations and final clean-source verification. Reverse62076 terminated0 at17:12:41UTC with all ten commands, five roster checks, two restorations and clean original-source verification passed; independent screen/reverse empirical and final documentation reviews approved. Raw evidence: b4-jobs-screen-1 and b4-jobs-reverse-1; failed helper preparation findings were fixed before either build sequence ran.

Parent-owned creation-checked process-tree sampling every30seconds retained18screen observations. Initial default edit:12samples, maximum sampled compiler/link working set6,581,161,984bytes, up to6rustc/3link, available memory at least16312MB, G:queue up to8. Jobs4 edit:2samples, maximum sampled2,640,261,120bytes, up to3rustc/1link, available at least18888MB, queue up to4. Different cache phases and sample counts prevent causal attribution. A first-screen link identity persisted across observations for at least62.573526s; this is a survival lower bound, not exact link duration. resource-sample-summary-v2.json preserves full creation-time identities. Reverse evidence adds8samples: edited default/jobs4 each2samples, sampled working-set maxima5,016,076,288/5,484,355,584bytes, rustc maxima3/4, linker maxima1/3, available minima17019/17110MB and queue maxima6/4. Sampling misses short phases and cannot establish true peaks or causation. True peaks/exact linker time remain NOT MEASURED.

The G: HDD shows sampled queues/paging, but a stable faster user-owned storage path was not tested or moved. Optional future relocation requires a separate capacity/cache-history comparison; no numerical SSD gain is claimed. Defender, power settings and unrelated processes were left unchanged.

## B4-B alternative linker gate (SKIPPED)

Canonical release edits remain library-codegen dominated: main compile-and-link mean16.735s bounds all work in that unit, about7%of239.982s wall time; actual linker-only cost is smaller or equal and unisolated. Broad test compilation exposes concurrent link/I/O activity, including the first-screen survival lower bound, but its first/replay cache asymmetry does not establish a worthwhile canonical-release linker change. The controlled warmed cohort completes in61.057936s under default scheduling; no measured alternative-linker gain or causal linker attribution exists.

Retain verified MSVC14.44.35207. Bundled rust-lld availability is not compatibility proof. No linker trial, shell override, global config, PDB/ABI experiment or alternative-native smoke was run. This is an evidence-led gate skip, not a failed LLD experiment or a claim that test linking cannot improve. A future trial should isolate material residual link time on the actual frequent workload before paying cache-population and native-validation costs.
## B5-A final compatibility and provenance (PASS)

Final production inputs are frozen at `4221c349acc34beb07d57852b70bfddbed0931ad` (F). The active `build-optimization` branch and owned scratch contain those inputs. The source audit from initial `6a6be519685652a282f7d0a411008601f158d075` confirms unchanged runtime sources, tests, resources, dependencies, lockfile and Cargo configuration. The only production changes are the two resource watches and the two named profile tables. Independent cumulative source/helper review approved that scope.

The final-input audit in `target/track-b/b5-frozen-input-audit.json` confirms that F differs from measured B2-B/B3 source `5819774e28491fead6d4634488969f60dc0115d7` only in an iteration-profile comment. B2-A source `7b6fab5e994be80875d18a09b0bc9ee80cc0b376` additionally lacks the later iteration table; its default/fast-dev settings and compiled source inputs are unchanged. The B5 runner revalidates prior large-edit success records, exact commands, source/target paths, fixture hashes, raw log hashes and effective manifest contracts before reusing either sample. Original measured SHAs remain attached to those observations.

Active and scratch checkouts have LF/CRLF differences, verified identical after normalization. Scratch fixture bytes retain their recorded hashes throughout comparisons; a final build from the active checkout is a separate path/cache transition. Toolchain recheck remains rustc 1.97.1, Cargo 1.97.1 and Nextest 0.9.135 on x86_64-pc-windows-msvc. No conflicting wrapper, Rust flag, profile, target or jobs overrides were found. Environment overrides remain possible for ordinary user commands; equivalent effective settings are required to reproduce these measurements.

Reuse the unchanged tracked-helper verification rather than run it again for documentation commits: B0-B tested exit 7, native arguments and paths with spaces, CWD/environment restoration and fresh timing capture; B1-B tested target/profile-selector guards, help, invocation outside the repository and native nonzero propagation. Helpers require PowerShell 7 and the relevant installed tools, remain optional, and do not require sccache, an alternative linker or editor configuration. Ignored campaign runners and raw artifacts are local evidence, not a portable committed automation framework.

B3 retains the exact 72-suite / 6,389-case / 17-ignored / two-empty-suite Windows roster under default and fast-dev, with identity mapping for every case. Its 108 scoped passes and B2 assertion/history evidence remain valid under the audited inputs. Ignored cases were listed, not executed. Full-suite execution, non-Windows validation, manual debugger inspection and full runtime-performance equivalence were NOT RUN.

Final active-checkout `cargo build --release --bin multi_launcher --timings -vv` passed at F in 1.042136 seconds, with the application Fresh and no compilation. This validates the documented canonical command and accepted original-source artifact after the completed scratch builds; it is excluded from edited benchmarks. Executable: 48,880,128 bytes, SHA256 `9d78d3893fea30addc0221685d8675e2b52144319eb28be7faf1e50d8b3fc5e5`. The first invocation was rejected before Cargo by the unchanged helper's outside-source evidence guard; the corrected run stores its manifest/logs under `C:/Users/Jay/.codex/visualizations/2026/10/10/01a1258c-086e-7d63-8e42-bdf503ac79a1/track-b-b5-active-release-1`. No source change or duplicate build was needed.

Original ICO/PE byte comparison passed in `target/track-b/b5-active-validation-1/icon-resource.json`. The identical executable SHA passed the bounded native smoke in `target/track-b/b5-release-native-1`: expected window observed, responsive WM_NULL, one posted WM_CLOSE, normal application exit 0, root gone, all recorded owned cleanup clear, no timeout. The smoke uses isolated owned settings/data and the existing inert hotkey setting; it does not prove full hotkey/plugin behavior or runtime-performance equivalence. Other three binaries retain B1's explicit compile evidence and B5's successful all-bin compilation, with unchanged source/targets. Named-profile PE/native and full-debug/assertion evidence are reused under the independently audited equivalent inputs.
## B5-B final comparison (IN PROGRESS)

The canonical release comparison uses the original-to-edited fixture, not an unchanged second command. Frozen F is `4221c349acc34beb07d57852b70bfddbed0931ad`; reused observations retain measured SHAs `5819774e` (B2-B/B3) and `7b6fab5e` (B2-A). Remaining all-bin, large-edit and branch-switch results will be filled only after the final runner succeeds. Initial populations, original-source preparations and source-path transitions are excluded from edited samples. The controlled branch workload uses create-only owned A/B commit refs and git switch --detach in the scratch worktree: A is exactly F; B changes only the recorded color fixture. It measures Cargo after each transition, not Git checkout latency, and leaves the active user branch untouched.

Nextest phases remain separate. Preparation values below have different cache/prerequisite histories and support no isolated speedup percentage. Cached invocations include command/runner overhead; they are not pure test execution.

| Nextest phase | B0 default seconds | Final default seconds | Final fast-dev seconds |
| --- | --- | --- | --- |
| Library original-source preparation, no-run | 210.186021 | 70.009479 | 167.773999 |
| Library cached execution, 8 passed | 2.009562 | Not remeasured in B3 | B2-A passing evidence reused; no B3 execution timing substituted |
| History preparation, no-run | 215.092202 | 226.245197 | 180.596997 |
| History cached invocation, 5 passed | 2.077256 | Not run in B3 | 3.326820 |
| Domain preparation, no-run | 20.862317 | 22.483401 | 8.785424 |
| Domain cached invocation, 102 passed | 2.796195 | Not run in B3 | 4.354443 |
| Broad compile/discovery, n=1 | 943.248524 | 938.288174 | 265.721831 |
| Already-built full roster listing | Not measured separately | 3.155210 | 2.963865 |

The separate B3 target-selection comparison measured cached no-run orchestration: unrestricted median 3.004230 seconds versus explicit-library 1.873911 seconds, three observations each, with no compilation. It does not replace B0's cached eight-test execution. Explicit target selection remains the architectural recommendation because a case-name filter alone does not constrain the Cargo build targets.

Matched B2-A compilation retains first/replay distinctions: launcher default/fast-dev 144.058486/93.992278 seconds first and 43.251080/30.412154 replay; library-test compilation 196.956480/175.760159 first and 40.201463/36.489082 replay. These are opt-in development-profile results, not canonical release improvements. Named-profile no-op overhead and extra cache population/storage remain documented above. Gains across different workloads must not be added.

Final all-bin edited observations at F: 315.332734 and 318.339727 seconds (median 316.836231), both exit 0. Baseline median 323.159398 seconds: observed 1.96% lower, with overlapping ranges and only two observations. This closest measured six-minute workload remains about 5.28 minutes; it is not proof of a substantial canonical release improvement. The original user command is unknown. Preparations 361.083072 and 311.385043 seconds are excluded; final original restoration and the remaining B5 gates are still running.

The first final all-bin sample's retained Cargo timing/verbose evidence confirms canonical flags on all five compiled application units: opt-level 3, strip=debuginfo and embed-bitcode=no, with no iteration/incremental/LTO override. The library took 221.6 seconds (frontend 60.0, codegen 161.6); binary units were radial_acceptance 92.7, launcher 19.6, coordinate smoke 6.4 and passive overlay 2.5 seconds. Units overlap, so their durations are not additive wall time or exact linker measurements. Root resource build-script/run units were 0.0 seconds, no build-script invocation appeared, and embed-resource was Fresh. The resource-watch path worked as intended for this unrelated source edit. Compact audit: target/track-b/b5-gaps-1/first-allbin-unit-audit.json.

Final canonical large-GUI repeat at F passed in 234.966570 seconds, after an excluded 0.979049-second fresh preparation. Combined with validated B2-B 238.326693 seconds, median 236.646632 seconds, range 234.966570–238.326693, n=2. Baseline median 240.309998 seconds: observed 1.52% lower, with overlapping ranges; no substantial canonical speedup is established. The identical GUI fixture and prior-input/raw-log revalidation support this combined final result, while retaining each actual source SHA.

The final iteration large-GUI repeat passed in 34.209382 seconds, after an excluded 98.381481-second original preparation. Combined with validated B2-B 28.489376 seconds: median 31.349379, range 28.489376–34.209382, n=2. The corresponding canonical large-edit median is 236.646632 seconds, so the observed named-profile comparison is 86.75% lower for this exact GUI fixture. This combined-profile result is distinct from canonical before/after performance and includes differing retained incremental histories; the preparation cost remains visible. Iteration branch switching was not separately benchmarked.

| Conditional opportunity | Final decision | Evidence / limitation |
| --- | --- | --- |
| Integration grouping | SKIPPED | No safe pilot with demonstrated payoff; retain all 67 targets and native/global-state isolation |
| Alternative linker | SKIPPED | Canonical main compile/link upper bound about 7%; linker-only time unisolated; MSVC retained |
| sccache | SKIPPED | Unavailable; no installation, hits/misses or branch-cache gain measured |
| Build jobs 4 | Tested; not retained | Reverse replay default 61.057936s versus jobs4 63.220442s; no repeatable benefit |
| Faster storage | NOT TESTED | G: HDD queues observed; no matched relocation or numeric SSD gain |
| Dependency features | SKIPPED | Warm dependencies Fresh; no identified expensive safe removal |
| Crate extraction | Analysis complete; DEFERRED | No stable costed boundary with demonstrated additional avoided recompilation |

The remaining canonical bottleneck is application-library codegen. Prioritize the explicit launcher target and opt-in iteration profile for daily work. Any later compiler, storage, linker or crate-boundary experiment should use a representative edit and matched cache histories; the present measurements do not justify a numerical forecast for those changes.
