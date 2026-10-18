# Track D execution ledger

Immutable baseline: `cbe104abc10ff03af2fd9897e88a4a331a7f4197`.
Starting branch: `temp`; clean checkout. Task branch: `perf-track-d`, verified
at the exact baseline. Origin: `multiplex55/Multi_Launcher` on GitHub.
No synchronization or cache deletion. Branch creation required sandbox escalation
because `.git` is read-only in the ordinary sandbox; approved retry succeeded.

Authority: `docs/plans/multi_launcher_perf_track_d_master_plan.md` and attached
kickoff. Active time includes builds. Caps: D0 50, D1 180, D2 140, D3 100,
optional D4 25, D5 45; total 540 minutes. Maximum two candidates and two variants
per candidate in each of D1–D3. No baseline full tests or early Clippy.

| Checkpoint | Status | Evidence / decision |
|---|---|---|
| D0-A | complete | Branch created and SHA verified; baseline_manifest.json |
| D0-B | complete | Existing runner reused; cargo --version dry run exit 0, 0.050 s. One setup correction: runner requires external evidence. |
| D0-C | complete | Deterministic manifest SHA256: 14cb37c63e2b42cbf4873ba83f5b61ed6fa5b43c6e9bccd309979c215791994a; materialization gated on isolation |
| D1-A | complete | Three command families; d1_baseline.json and developer_loop.ps1 |
| D1-B | complete | d1_attribution.md; headless RA timed out at 60.07 s |
| D1-C/D | complete_keep_recipe | Existing fast-dev: edits save 15.05/20.79 s; functional guard PASS |
| D1-E/F | skipped_no_candidate | No independent measured small owner |
| D1-G | complete | D1 complete with confirmed existing recipe; no production optimization |
| D2-A | complete | Test-only sample override; bound/default test PASS, native containment blocked |
| D2-B | partial_environment | Query measured; native startup/hotkey blocked |
| D2-C | blocked_environment | Dashboard/radial native timing unavailable; no substitute claim |
| D2-D | complete | Rank internal observations, insufficient evidence for root-cause candidate |
| D2-I | complete_partial_environment | CPU baseline retained; native categories explicitly unmeasured |
| D2-E–H | skipped_insufficient_evidence | Zero candidates/variants; no native guardrail or release attribution |
| D3-A | complete_inspection_only | d3_owners.md; no runtime counter added |
| D3-B | blocked_environment | Four controlled GUI idle scenarios cannot be safely launched |
| D3-C–F | skipped_insufficient_evidence | No measured whole-process bottleneck |
| D3-G | complete_blocked_environment | Four unrun scenarios with null rates in d3_idle.json |
| D4-A/B | skipped_optional | Required native ownership/containment unavailable; no memory observation |
| D5-A | complete | Diff/metadata reviewed, two existing default-protocol tests PASS |
| D5-B | complete | results.md, per-track evidence and at most three follow-ups |
| D5-C | pending | Touched Rust formatting; Clippy last |

Raw evidence: external `$env:TEMP/Multi_Launcher/perf-track-d/`; `target/perf-track-d/` reserved for owned fixtures. No screenshots collected, no
optimization candidates attempted, no application processes launched yet.

`D0-B`: about 2.5 active minutes through runner dry run; no optimization experiments. Toolchain: Rust/Cargo 1.99.0, Nextest 0.9.135, Windows 10.0.19045 x64, 8 logical processors. CIM OS/CPU inventory denied; RAM/power unmeasured. Rust Analyzer 1.99.0 available. Files: ledger, baseline manifest, runner recipe. No Rust source changed.

`D0-B` commit: a3bf2959; push rejected by automatic approval review: external destination/payload trust not established. No bypass/retry. D0-C: counts, event semantics and safety boundaries pinned in fixtures.json. D0 total 2.9 active minutes (18:08:01–18:10:59 UTC). D1 starts next; no optimization candidates yet.

Clock correction: initial approximate D0 durations were overestimates; commit/run timestamps establish D0 at 2.9 minutes. D1 began 18:10:59 UTC. Check initial preparation 69.94 s; no-op 0.96/1.00/0.97 s; history edit 12.34 s; GUI edit 12.26 s. Cargo application check unit: 11.38/11.20 s. Exact byte restoration succeeded. D0-C commit 6e945b88; subsequent pushes held because the same destination/payload trust rejection remains in force (no bypass).


D1-A finished 18:22 UTC (~11 minutes into D1; total ~14 minutes). App initial preparation 250.81 s; no-op 1.06/1.19/1.13 s; edits history 42.71 s, GUI 47.61 s. Test edits 46.90/45.89 s; test case PASS 0.028 s, Nextest run 0.039 s (full command 47.21 s includes restoration rebuild). Fixtures restored byte-for-byte; no product source diff. Compile/check and exact selected library test passed. Parser check and harmless summarizer run passed. No cold-cache build requested or cache deleted. Only existing cache used. Summary fingerprints include source SHA, command/profile/cache; early samples lack per-file hashes but exact appended comment recipe and restoration are recorded. Raw logs remain external. Candidates/variants: 0/0. Push held under prior auto-review rejection.

D1-A commit 030e4868 (local). D1-B: RA database load 25.94 s, 1283 MB reported internally; headless history analysis did not finish in 60.07 s and its owned process tree was killed/disposed. RA_NOT_MEASURED for interactive response; no editor config changed. D1 elapsed 13.1 min at 18:24:05 UTC. Candidate 1 starts after attribution commit, max 55 min; current variants 0. Push remains held under prior automatic-review rejection.

D1-C candidate 1 clock starts 18:24:19 UTC, deadline 19:19:19 UTC. Variant 1 is existing fast-dev; no Cargo.toml edit. Its previously absent target/fast-dev cache is the one new-profile cache population in D1; existing dev/release caches retained. Baseline C/D paired workloads are history and GUI comment edits, with >=15% AND >=10 s needed in each comparison; no-op guard tracked independently. Candidate 2 remains skipped. No CARGO_PROFILE_*, CARGO_INCREMENTAL, RUSTFLAGS, CARGO_ENCODED_RUSTFLAGS or CARGO_TARGET_DIR environment overrides were present.

Bounded read-only reviewer pass found stale-session mixing risk in the summarizer; corrected with required UTC window and source/command grouping. Rebuilt original baseline summary using 18:10:59–18:22:40 UTC: 19 samples, exactly 3 per no-op group, same numeric observations. Corrected missing-hash limitation to all original baseline samples. No extra Cargo run. This is the single evidence-tool correction for this checkpoint.

D1-D completed ~18:33 UTC: one candidate/one variant, ~9 minutes incl guard. Two edited pairs save 35.24%/43.66% (15.052/20.790 s). No-op +0.229 s /20.32% documented guard exception, outweighed by recurring edit gain. Existing opt-in recipe retained; no product source or profile changed. Exact fast-dev test guard PASS 0.032 s, Nextest run 0.050 s. d1_results.md, d1_profile_comparison.json and corrected baseline/summarizer are evidence. D1-B commit fe84cd10 local; pushes held under previous rejection. Review findings resolved; no extra review loop.

D1-G closure 18:33 UTC: approximately 22 active minutes of 180; one candidate, one variant, no new production optimization. Current-checkout recipe documented under dev/perf_track_d. D1-D commit 75b38c12 local. An extra blank EOF line reported by diff-check during D1-D was removed in this closure. D2 starts next; overall remaining budget approximately 515 minutes. No owned GUI process or screenshot yet.

D2 began 18:33:36 UTC. D1-G commit 06f5e0b5 local. Native normal attempt: existing owned-process runner with harmless cargo --version returned exit 1, reason containment monitor failure: Access denied; cargo exit 0, root absent, cleanup verified. One diagnostic retry: exact current-PID CIM query also access denied. Computer Use initialized and window inventory succeeded; no screenshots/input/user content collected. Native GUI fixtures not launched because required descendant containment cannot be established. Native retry allowance exhausted; do not bypass via another process runner. Test-only sample override planned for existing isolated Track C measure_owner/emit_samples, default 20 preserved, Track D requests 15; no runtime allocations or production instrumentation changes.

D2-A: dedicated sample-bound/default unit test PASS (0.016 s case, 0.026 s test run; full build command 67.42 s). Rustfmt applied only to touched test fixture file. Production module excludes this helper with cfg(test). Existing hand-rolled Track C protocols remain unchanged and will not be run at 20 samples. No new executable, observer, production dependency or runtime polling. D2 elapsed ~2.5 min; candidates 0, variants 0. Docs show the exact limited CPU-only path.
D2-B: query CPU-frame exact ignored test PASS; command wall 89.11 s includes fixture setup outside timed owners. 5 warmups +15 measurements for each 100/1000/10000 synthetic catalogs. p50/p95 ms: 1.0947/4.2427, 1.6240/2.9170, 6.2323/6.7161. Stable outputs/receipts validated by existing oracle. Instrumented fast-dev CPU-only, overhead unmeasured; no end-to-end or release claim. Source 78f5b16c (D2-A committed); no query text or user content in source-controlled evidence. D2 candidates remain 0.

D2-C/D: d2_results.md records all five core categories with null/unmeasured native metrics. Updated fixture manifest v2 pins actual reused 100/1000/10000 CPU catalogs; original native fixtures remain planned only. Manifest SHA256 1cc076d159fd8a1580d12cfbd3db391c2205db398ab2b146ac307357b1cf47ca. D2-B commit f2f59779 local; push held under prior rejection. No Criterion run without causal trace; no synthetic result mislabeled native.

D2-I closure 18:39 UTC (~6 active minutes of 140; overall ~31 min, ~509 min remaining). D2-D commit d1379783 local. Native capability summary added with true null metrics and verified harmless-probe cleanup. Zero UI optimization candidates, zero variants, zero screenshots. Only implementation is test-only sample control, validated by its focused unit and exact existing query test. No new default binary or production behavior. D3 starts next; no native retries will be repeated while the established containment boundary is unchanged.

D3 clock starts ~18:39:20 UTC; D2-I commit cd03ae95 local. Bounded inspection mapped Hot Folder, gesture, cursor/click and radial scheduling owners. Process CPU/wakeups/memory/handles/threads not sampled: same required containment restriction proven in D2. No repeated native retry or new polling instrumentation. D3 candidates/variants 0/0; no source edits or tests appropriate for inspection-only work. Push held under prior automatic-review rejection.

D3-B/G closure 18:41:09 UTC (1.55 active minutes of 100): all four requested idle scenarios explicitly BLOCKED_ENVIRONMENT; actual measured duration 0, rates and memory/handles/thread/watcher counts null. No before/after, no 300 s confirmation, no responsiveness claim, no candidate/code accepted. D3-A commit c27164c9 local. D4-A gating: SKIPPED_OPTIONAL because required native containment/ownership is unavailable, despite ample time. D4-B not run, no growth/leak claim. Proceed directly to D5; no owned GUI processes need cleanup. Overall 33.13 active min, ~506.87 min remaining. Pushes remain local under prior auto-review rejection.


D5 starts at D3-G commit a8f994e9, 18:41:09 UTC. Cumulative diff inspected: only dev scripts/recipes, sanitized performance docs, and cfg(test)-only sample-limit helper. No product settings/hotkeys/input/shutdown/release/Cargo changes. cargo metadata --no-deps --offline --format-version 1 PASS: sole workspace member multi_launcher; sole ungated application bin multi_launcher; three existing smoke bins still require dev-smoke-binaries. No new binary/dependency. Exact two existing default-protocol integration unit tests PASS (measurement validation preserves all 20 default samples and revision receipts), command wall 1.99 s, Nextest 0.027 s. No full suite or unrelated plugin tests. Existing query test already validated configured 15 samples. No native GUI process ever launched; harmless root probe verified absent, RA owned tree terminated/disposed. Git status contained only intentional task files before this checkpoint.

Clock reconciliation uses actual commit/run timestamps: D0 18:08:01–18:10:59 (2.97 min); D1 18:10:59–18:33:29 (22.50 min); D2 18:33:29–18:39:36 (6.12 min); D3 plus optional D4 decision 18:39:36–18:41:09 (1.55 min). Earlier rounded updates are approximate; these intervals govern accounting. Baseline manifest now distinguishes task clock start from manifest creation. D5 cap deadline 19:26:09 UTC, overall cap 03:08:01 UTC next day. No user pauses excluded.

D5-B report finalized before final lint, including all local checkpoint hashes, no accepted production optimization, confirmed existing recipe benefit/no-op exception, CPU-only query baseline, null native/idle results, optional memory skip and three future follow-ups. Corrected settings_hash fields to null where settings-object hashes were not measured; kept deterministic fixture signatures/manifest hash explicitly labeled. D5-A commit 3abf406f local. Next: touched Rust formatting, then exactly one maintained-workspace all-target Clippy sweep; no additional tests afterward unless an owned lint correction requires it. Pushes remain held under the same automatic-review denial.
