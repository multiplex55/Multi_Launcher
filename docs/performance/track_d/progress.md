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
| D1-A–G | pending | Developer loop measurements |
| D2-A–I | pending | Responsiveness |
| D3-A–G | pending | Idle |
| D4-A/B | pending | Optional observation |
| D5-A–C | pending | Focused closure; Clippy last |

Raw evidence: external `$env:TEMP/Multi_Launcher/perf-track-d/`; `target/perf-track-d/` reserved for owned fixtures. No screenshots collected, no
optimization candidates attempted, no application processes launched yet.

`D0-B`: ~7 active minutes through runner dry run; no optimization experiments. Toolchain: Rust/Cargo 1.99.0, Nextest 0.9.135, Windows 10.0.19045 x64, 8 logical processors. CIM OS/CPU inventory denied; RAM/power unmeasured. Rust Analyzer 1.99.0 available. Files: ledger, baseline manifest, runner recipe. No Rust source changed.

`D0-B` commit: a3bf2959; push rejected by automatic approval review: external destination/payload trust not established. No bypass/retry. D0-C: counts, event semantics and safety boundaries pinned in fixtures.json. D0 total approximately 8 active minutes. D1 starts next; no optimization candidates yet.
