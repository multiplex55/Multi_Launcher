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
| D1-C/D | in_progress | One existing fast-dev workflow comparison; no product source edits |
| D1-E/F | skipped_no_candidate | No independent measured small owner |
| D1-G | pending | Close developer loop findings |
| D2-A–I | pending | Responsiveness |
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
