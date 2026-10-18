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
| D2-B/C/D/I | pending | Internal evidence and explicit native limitations |
| D2-E–H | pending | Candidates only after baseline |
| D3-A–G | pending | Idle |
| D4-A/B | pending | Optional observation |
| D5-A–C | pending | Focused closure; Clippy last |

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
