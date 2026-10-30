# Kickstart — G05 B1 terminal byte accounting

Read [GOAL](GOAL.md), current G05 contract/amendment, B1 handoff and R1 review. Parent selects valid checkpoint and authorizes one bounded correction.

1. Inspect LiteralScan::feed/finish and oversized/terminal tests; establish supplied-versus-processed counterexample.
2. Correct accounting/finalization without lifting limits or reviving failures.
3. Add deterministic resource/cancel/deadline EOF cases and keep real source-change/overrun/prematureEOF controls, both modes.
4. Run only approved focused correction; preserve exact original and corrected receipts.
5. Return for R1-only confirmation; all configured-condition gates remain closed.

No reader/native/B2, automatic renewal or independent dispatch. Parent owns checkpoint/push/integration. Unresolved R1 keeps B1 unaccepted; resolved case not separately scheduled.
RESOLVED_IN_G05 at944f9d0e, named independent confirmation. Historical procedure only; do not execute/schedule.
