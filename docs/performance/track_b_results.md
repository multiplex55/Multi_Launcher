# Track B results

Baseline: cargo-build-perf at `6a6be519685652a282f7d0a411008601f158d075`. Implementation: build-optimization; local commits only, no push/merge.

B0 is frozen; source/profile configuration is unchanged. No final improvement claim yet. Full protocol and cache caveats are in track_b_baseline.md.

| Scenario | Baseline seconds | Final | Comparison constraint |
| --- | --- | --- | --- |
| No-op release launcher | median1.017300; range0.997221–1.058698; n=3 | NOT MEASURED | Fresh, identical command |
| Small edited release launcher | median244.682906; range239.562765–249.803047; n=2 | NOT MEASURED | Same original-to-color-edit workload |
| Large edited release launcher | median240.309998; range237.433298–243.186698; n=2 | NOT MEASURED | Same original-to-GUI-edit workload |
| Default all-bin edited release | median323.159398; range317.662238–328.656558; n=2 | NOT MEASURED | Same small edit; four binaries |
| Branch-switch release A→B / B→A | medians243.453353 / 240.954734; n=2 each | NOT MEASURED | Owned refs, exact same color change |
| Library Nextest preparation / cached invocation | 210.186021 / 2.009562 | NOT MEASURED | First target preparation; 8 passed |
| History preparation / cached invocation | 215.092202 / 2.077256 | NOT MEASURED | 5 passed; normal Cargo prerequisites |
| Domain preparation / cached invocation | 20.862317 / 2.796195 | NOT MEASURED | 102 passed; prior prerequisites reused |
| Broad Nextest compile/discovery | 943.248524; declared6389cases | NOT MEASURED | Inventory only; no full-suite execution |
| Check target population / warm checks | 76.682837 / 1.078176,0.918521 | NOT MEASURED | Keep first population separate |
| Named iteration profile | NOT MEASURED | NOT MEASURED | Population/edit/no-op separate |

The all-bin median was78.476492s slower under the same edit; extra targets are the intended workload difference. Sequential trials and differing library durations prevent attributing every second solely to target selection. The all-bin baseline is closest to the approximate six-minute report; original user command is not established. No compiler change has yet been measured.

Conditional gates: broad test linker/memory observations justify reassessing one jobs4 experiment after reduced symbols; alternative linker only if material residual cost persists. sccache unavailable; dependency pruning lacks demonstrated expensive-dependency evidence; broad crate extraction remains deferred. These decisions will be finalized at B4, not presumed accepted now.

B0 PE/icon verification passed against original ICO after branch restoration. Inventory:72suites,6389cases,17ignored,2empty; zero duplicate logical cases. Native startup remains NOT RUN. Independent baseline/support review approved after causal-attribution wording and fresh-output collision fixes.

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
