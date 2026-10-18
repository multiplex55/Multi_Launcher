# Performance Track D results

**No new production optimization met the acceptance gate.** The existing
fast-dev workflow was revalidated with meaningful edited-build savings. Native
responsiveness and idle work were blocked at required process containment;
those results are unmeasured, not passes. Optional memory observation skipped.

Branch `perf-track-d`, created from exact clean starting HEAD
`cbe104abc10ff03af2fd9897e88a4a331a7f4197` on `temp`. No fetch/pull/merge,
history rewrite, user changes or existing cache removal. D5-A checkpoint:
`3abf406f`. The final D5-C tip is identified in the chat handoff / branch log.
All checkpoints are local. First push to configured origin was rejected by
automatic approval review because external destination/payload trust was not
established. Later pushes were held under that same denial; no retry/bypass.

| Track | Result | Measured benefit / limitation |
|---|---|---|
| D1 | Existing opt-in recipe confirmed | Edited dev 45.160 s median vs fast-dev 27.239 s: 17.921 s /39.68% saved; two distinct edit pairs |
| D2 | Partial environment coverage | Query CPU-only p50/p95: 100 actions 1.095/4.243 ms; 1k 1.624/2.917 ms; 10k 6.232/6.716 ms |
| D3 | BLOCKED_ENVIRONMENT | Four planned controlled idle cases unrun; all rates/counts null |
| D4 | SKIPPED_OPTIONAL | Required safe native containment unavailable; no memory-growth claim |

Implemented: one script family reusing Track B measurement/containment owners,
bounded summary windows, synthetic fixture/evidence manifests, and an optional
test-only sample-count override (default 20 preserved, Track D requested 15).
Only Rust change: `src/performance/track_c_workloads.rs`, included with cfg(test).
No production observer/worker, new binary, dependency, profile or release change.
No measured runtime optimization accepted; no rejected source optimization
needed rollback. D1 used one candidate/one variant; D2/D3 used zero. One bounded
read-only tooling review found summary mixing/missing-hash wording issues; both
resolved without a review loop.

D1's two edited pairs saved 15.052 s /35.24% and 20.790 s /43.66%. Both pass
the >=15% AND >=10 s gate. No-op median regressed 0.229 s /20.32%, exceeding
the relative guard; explicitly accepted for the **opt-in recipe** because each
measured edit saves 15–21 s. Full-symbol debugging remains ordinary dev.
Fast-dev preparation 234.33 s is separate cache population, not an edited gain
or a matched cold-build comparison. Native startup/runtime behavior under this
profile was not validated. The selected functional test passed in both profiles.
See d1_results.md and per-sample JSON for attribution, caches and variance.

D2 uses existing isolated Track C app/query CPU-frame owners, 5 warmups +15
measured per catalog, validated complete output/revision receipts. Source
`78f5b16c23d2f461918bab5216954bbd59351410`, fast-dev, default features,
960x640 scale 1.0, warm fonts/cold query geometry. Raw durations and deterministic
fixture identities: d2_query.json. Settings object hash unmeasured; static
settings label and source-pinned constructor recorded. Instrumentation overhead
unmeasured (`instrumented_only`); 15-sample nearest-rank p95 is the maximum.
These are unoptimized CPU timings, not compositor/native-visible/release
latency, and do not justify a root-cause product change. No startup, hotkey,
dashboard, radial or submenu presentation metric is claimed. No screenshots
(0/16), native fixture launch, keyboard/mouse input or clipboard access.

Required native runner failed its harmless `cargo --version` probe with access
denied on process inventory. Root exited 0 and was verified absent. One exact
current-PID diagnostic confirmed denial. Computer Use window inventory worked;
it did not establish descendant containment. No alternate runner used to bypass
that boundary. D3 reuses this unchanged blocker, inspects four scheduling owners
only, and leaves Hot Folder's 80 ms interval unchanged. D4 was not started.
RA loaded a database in 25.94 s but bounded headless analysis timed out at
60.07 s; owned process tree terminated. Interactive RA response unmeasured.

Host: Windows 10.0.19045 x64, 8 logical processors; rustc/Cargo 1.99.0 MSVC,
Nextest 0.9.135, RA 1.99.0. RAM/CPU model/power inventory denied. No Cargo/Rust
profile/flags environment overrides present during D1. Early baseline per-file
hashes unavailable; exact controlled-comment recipe and byte/hash restoration
recorded. Subsequent candidate records include per-file hashes. Raw logs stay
outside source in `$env:TEMP/Multi_Launcher/perf-track-d`; summaries contain no
private paths, source text, window titles or clipboard data.

Verification actually performed:

- Existing runner harmless dry run, parser and bounded summarizer checks passed.
- Selected `cargo check --lib`, app build, and exact filtered Nextest build/run
  in dev; selected app build and exact history test in existing fast-dev passed.
- New sample-bound/default test passed; existing ignored query CPU-frame test
  passed with exactly 15 samples per size.
- Two exact existing measurement/default-protocol unit tests passed in D5
  (20 arithmetic-owner samples each; not additional UI interactions).
- `cargo metadata --no-deps --offline --format-version 1` passed: only maintained
  workspace member multi_launcher; only ungated app binary multi_launcher;
  three existing smoke binaries still gated by dev-smoke-binaries.
- Cumulative diff inspected; source fixtures restored, no user changes, only
  test-only source instrumentation. No whole Nextest suite required or run.
- Touched Rust formatting passed: rustfmt --edition 2024 --check src/performance/track_c_workloads.rs.
- Final cargo clippy --workspace --all-targets --offline -- -D warnings failed (exit 101, 137.95 s): existing clippy::let_and_return at src/paste_sequence/playback.rs:597 and src/gui/command_host.rs:420. Both files are unchanged from the baseline, verified by empty path diff and baseline source inspection. No new owned lint reported; remaining sweep is not claimed clean. No unrelated fixes or second Clippy run.

Reproduce selected workloads using `dev/perf_track_d/README.md`; do not broaden
them to a historical acceptance matrix. Active clock began 18:08:01 UTC on
2026-10-18. Commit/run reconciled D0 2.97 min, D1 22.50 min, D2 6.12 min,
D3 plus D4 skip decision 1.55 min; D5 started 18:41:09 UTC (45-minute cap).
Overall hard ceiling 540 minutes; no candidate approached its per-track cap.
D5-C closed at approximately 18:47:30 UTC: total active elapsed about 39.5 minutes, D5 about 6.35 minutes, all below caps. No user pauses excluded. Final validation has the precise Clippy limitation above.

Bounded future backlog (outside this completed pass):

1. Restore access to required owned-process containment before native UI/idle
   profiling; highest benefit is trustworthy end-to-end and idle attribution.
2. If profiling is resumed, compare release query path and instrumentation cost
   before interpreting the 10k diagnostic result as a product hotspot.
3. Attribute Hot Folder zero-rule whole-process CPU/publication cost before
   considering an idle-only scheduling change.

Checkpoint commits (all local; subjects are stage-tagged in `git log`):

| Stage | Commit | Outcome |
|---|---|---|
| D0-B | a3bf2959 | Exact baseline and reused runner |
| D0-C | 6e945b88 | Planned synthetic workload manifest |
| D1-A | 030e4868 | Developer-loop baseline |
| D1-B | fe84cd10 | Attribution and bounded recipe trial |
| D1-D | 75b38c12 | Existing fast-dev recipe comparison |
| D1-G | 06f5e0b5 | Developer-loop closure |
| D2-A | 78f5b16c | Test-only sampling control |
| D2-B | f2f59779 | Query CPU receipts |
| D2-D | d1379783 | Native blocker and query ranking |
| D2-I | cd03ae95 | Partial responsiveness closure |
| D3-A | c27164c9 | Idle ownership inspection |
| D3-G | a8f994e9 | Blocked idle / optional memory skip |
| D5-A | 3abf406f | Focused protocol/target verification |
| D5-B | f8137e27 | Final report before last formatting/lint |
