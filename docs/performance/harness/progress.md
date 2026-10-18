# Benchmark harness execution ledger

Baseline: `3a989d56abe1b203902e0c6eb6075ae58f6235ef`; starting branch
`perf-track-d`; clean tracked/untracked status. Created `perf-harness` directly
at that SHA. Origin is `multiplex55/Multi_Launcher`; no history synchronization.

Started 2026-10-18. Absolute budget 260 minutes; stage caps 20/75/95/35/35.
Only benchmark infrastructure is authorized; production optimization is excluded.

## H0-A — complete

Commands: `git rev-parse HEAD`, `git branch --show-current`, `git status --short`,
`git remote -v`, upstream query, `git switch -c perf-harness <baseline>`.
The sandbox initially denied `.git/index.lock`; normal approval allowed branch
creation. No user changes were present. Read current AGENTS.md and master plan.
Cargo metadata before additions: ignored `target/perf-harness/metadata-before.json`.
No native execution or performance metrics yet. Track D reports moved into
`docs/archive/performance/track_d/` in this checkout.

Checkpoint commit/push results are recorded by subsequent ledger entries.

## H0-B — complete

Contract: `contract.md`. Inspected manifest, existing owned runner, Track B
argument-array measurement and archived Track D denial/owner map. Radial's
existing public production paths provide the CPU success candidate; private
worker/widget owners require bounded access inspection before implementation.
H0-A commit `b90182c3` pushed successfully. No measurements claimed.

## Pending

H1-A/B/C; H2-A/B/C; H3-A/B; H4-A/B/C.

## H1-A — partial (implementation complete; focused test compile pending)

Added nondefault feature, gated runner receipt stub and shared recorder with
strict CLI, bounded/unique output, per-run reservations for nested recorders,
terminal events, atomic summaries, null reasons and four focused tests.
`cargo check --features perf-harness --bin perf_probe_runner` passed exit 0
(17.68 s) before final small review fixes. `rustfmt` and `git diff --check`
passed. Focused Nextest compilation is still running; no tests claimed passed.
H1-C will finish verification against final source. No broad suite or Clippy.

Read-only review found three receipt defects, all corrected: metadata is now
inside the 32 MiB run cap (30 MiB stream allowance); metric sets/units validate
before mutation; returned and persisted failure summaries retain identical
reason/timestamps. Windows reparse paths are rejected. H1-A's 20-minute edit
budget ends here; pending compilation does not justify more recorder features.

## H1 observations (in progress)

H0-B commit `19bb2fa3` pushed. Source inspection confirmed Hot Folder can use
an internal feature-only driver bridge and isolated `AppDataRoot`; dashboard
needs an inert plugin factory, because ordinary construction installs shared
macro runtime. Radial public geometry/scene APIs need no production bridge.

Required legacy capability attempt (once):
`& ./dev/run_owned_process.ps1 -Executable (Get-Command cargo.exe).Source -Arguments @('--version') -TimeoutSeconds 3 -EvidenceDirectory target/perf-harness/legacy-capability`.
Runner exit 1; exact receipt reason `containment monitor failure: Access denied `;
child exit 0, RootAlive false, PostCleanup `Absent`. Raw evidence is ignored at
`target/perf-harness/legacy-capability/result.json`. No repeated CIM diagnosis.
This does not establish Job Object capability or desktop access.
