# Branch chain

One straight chain from fetched master. Each first stage commit has the exact previous closing commit as parent; there are no merges, rebases or cherry-picks. All B1-B8 closing refs equal their pushed origin refs. B9 is the only PR head.

| Stage | Branch | Creation parent | Closing tip | Push / ancestry |
|---|---|---|---|---|
|B1|codex/linear-01-g02-table-publication-20261030|f28355df5e210cfc1633284b0b80c7f156090245|f26ba48e73657d1ab033884bf11e7731b7380646|pushed; PASS|
|B2|codex/linear-02-g05-hotfolder-20261030|f26ba48e73657d1ab033884bf11e7731b7380646|c6019ac581fbc409a24a7c9693efc7ff660e1549|pushed; PASS|
|B3|codex/linear-03-g03-table-full-file-20261030|c6019ac581fbc409a24a7c9693efc7ff660e1549|5638e107df2e09bbf4f86a622af3cc15205b3af2|pushed; PASS|
|B4|codex/linear-04-g06-jump-native-20261030|5638e107df2e09bbf4f86a622af3cc15205b3af2|16564925a82c464e351f8e9936db61383e3a1241|pushed; PASS|
|B5|codex/linear-05-g08-mkmacro-history-20261030|16564925a82c464e351f8e9936db61383e3a1241|0bc964572f38c7b18cc790e7fa9da0f48df469ef|pushed; PASS|
|B6|codex/linear-06-g04-screenshot-native-20261030|0bc964572f38c7b18cc790e7fa9da0f48df469ef|b00365b30cee011a049fd836260d720372b35f02|pushed; PASS|
|B7|codex/linear-07-g07-zip-extraction-20261030|b00365b30cee011a049fd836260d720372b35f02|ee59a328a490b73589243e7c1acdb58fb763055b|pushed; PASS|
|B8|codex/linear-08-g08-recipes-policy-20261030|ee59a328a490b73589243e7c1acdb58fb763055b|4bc5c4b156936135c0d3f5421512a7ceeb57b4b3|pushed; PASS|
|B9|codex/linear-09-final-review-20261030|4bc5c4b156936135c0d3f5421512a7ceeb57b4b3|[live closing ref](https://github.com/multiplex55/Multi_Launcher/tree/codex/linear-09-final-review-20261030)|M0/M2 pushed; final reporting push resolved by PR/ref; PASS|

[Machine proof](stages/B09-final/BRANCH_PROOF.json), [B9 predecessor receipt](stages/B09-final/PREDECESSOR_RECEIPT.json). Closing B9 SHA is reported by the Git ref/PR receipt after commit; this file cannot contain its own containing commit SHA. Historical predecessor receipts are retained under each stage.

B7 required corrective M0 documentation commits after an encoding error; no writer was dispatched until the full packet was verified. No history rewrite.

One cumulative [PR #1460](https://github.com/multiplex55/Multi_Launcher/pull/1460), base master, unmerged. [Publication snapshot](stages/B09-final/PUBLICATION_SNAPSHOT.json) predates the containing final reporting commit; final SHA is resolved by the live branch/PR and final response. The machine proof records independently verified predecessor and review-checkpoint hashes without inventing a self-referential closing SHA.
