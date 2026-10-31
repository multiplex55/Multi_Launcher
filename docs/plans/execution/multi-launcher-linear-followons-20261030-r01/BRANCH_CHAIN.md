# Branch chain

| Stage branch | Creation parent | Closing tip | Push / ancestry |
|---|---|---|---|
| B1 codex/linear-01-g02-table-publication-20261030 |f28355df5e210cfc1633284b0b80c7f156090245|f26ba48e73657d1ab033884bf11e7731b7380646|M0 and M3 pushed; origin/master ancestor PASS|
| B2 codex/linear-02-g05-hotfolder-20261030 |f26ba48e73657d1ab033884bf11e7731b7380646|c6019ac581fbc409a24a7c9693efc7ff660e1549|M0/M3 pushed; ancestor PASS|
| B3 codex/linear-03-g03-table-full-file-20261030 |c6019ac581fbc409a24a7c9693efc7ff660e1549|5638e107df2e09bbf4f86a622af3cc15205b3af2|M0/M3 pushed; ancestor PASS|
| B4 codex/linear-04-g06-jump-native-20261030 |5638e107df2e09bbf4f86a622af3cc15205b3af2|in progress|initial HEAD equality and ancestor PASS|

B2 was created at B1's exact closing tip after clean-status and process-drain checks. Predecessor name/SHA/push were recorded before switch in the owned runtime receipt, copied into [B2 receipt](stages/G05/PREDECESSOR_RECEIPT.json). Closing SHA is materialized in the following stage's M0 ledger to avoid inventing its containing commit's hash. No merge, rebase, cherry-pick, force push or intermediate PR. B5..B9 remain pending and must use the same predecessor-tip rule.

B3 predecessor receipt: [B2 to B3](stages/G03/PREDECESSOR_RECEIPT.json).

B4 predecessor receipt: [B3 to B4](stages/G06/PREDECESSOR_RECEIPT.json).
