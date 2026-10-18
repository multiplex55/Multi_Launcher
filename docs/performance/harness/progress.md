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
