# Final manual review checklist

This is the single optional final user review checklist for the eventual integration PR. It does not authorize deferred implementation, grant a new verification budget or waive an acceptance gate.

- [ ] Review the [included source/import manifest](INTEGRATION_MANIFEST.md) and excluded G02/G07/G08 prototypes; confirm the PR contains the intended accepted scope.
- [ ] Review accepted Diff and Jump editor/transaction behavior, while distinguishing G05's gated foundations from enabled condition execution. No condition activation is included.
- [ ] Review [remaining native limits and canonical future work](FOLLOW_ON_CATALOG.md), then explicitly select any desired next work. G04's [isolated pixel protocol](goals/G04/follow-on/native-gesture-output-proof/KICKSTART.md) and G06's [isolated native Jump protocol](goals/G06/follow-on/native-jump-focus-proof/KICKSTART.md) remain NOT_RUN native acceptance, not required mid-goal user tests or waivers.
- [ ] Before any merge, verify the actual PR head/base, host-reported checks and review state in [final handoff](FINAL_HANDOFF.md). These are currently PENDING; no automatic merge or branch deletion is authorized.
