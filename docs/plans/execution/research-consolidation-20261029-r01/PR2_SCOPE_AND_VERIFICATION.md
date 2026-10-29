# M2-C — docs scope and bounded verification

Actual topology: **STACKED_ON_UNMERGED_PR1**, head branch `codex/integrate-research-docs-20261029-r01`, base branch `codex/integrate-query-escape-20261029-r01`, base SHA `ed25f9dcc32f9bb74d2277d25d6fb62e292636c6`. Master is still `59156d96adcc73939c7171487a0cb582e3286587`. PR1 is #1456 OPEN; automatic approval review blocked normal merge. PR2 is not a master-ready docs diff until PR1 merges and it is retargeted/rechecked.

M2-A `6f8de6cfa4d1e8bc82c2eabac41fb43299402049` and M2-B `3f65d2eb20e68b592d2735a56d429d863eecd43f` are pushed. [Manifest](INTEGRATION_MANIFEST.md) and its [exact file/source/blob map](receipts/integration-imports.json) cover 171 unchanged imports. Additional files are integration receipts and the active Goal's required mutable section only. No source/config/test delta against the actual base is allowed. Historical research and decision claims remain pinned to their original source; old checkers are not rerun against the query-updated integration source.

One standard-library metadata checker is prepared at `receipts/check_docs.py`. The parent runner enforces 60 seconds; there is one initial pass and at most one localized correction/recheck. It checks current imported Git blobs, docs-only scope, local links/known anchors, strict JSON/JSONL parseability and preserved counts/status/version labels. It never imports/evaluates prototype or application modules, accesses network or mutates source data. Preserved BOM/newline semantics remain source evidence. External/pinned-source URLs are not refetched; original source hashes and historical citation checks remain preserved, not relabeled new checks.

At this receipt's creation, the formal metadata pass and separate PR2 review are **PENDING**, with zero passes consumed. Actual output, elapsed/budget, failures/corrections if any, review disposition and PR2 host data will be appended before publication. No new Cargo/rustc/Nextest/Clippy/rustfmt/native/app/benchmark/provider/dependency/user-data operation occurred. No full-suite claim or native guarantee is made.

## Initial metadata result — 2026-10-29

**PASS: 1,497 assertions**, zero failures. [Checker output](receipts/docs-check-initial.json) and [enforced-runner receipt](receipts/docs-check-initial-runner.json) record checked M2-B HEAD `3f65d2eb20e68b592d2735a56d429d863eecd43f`, 171 exact source imports, 137 Markdown files, 924 local links/63 relative-anchor links, 34 JSON files and 14 JSONL files (4,200 preserved records). Both 300-case corpus versions, the 1,800 original predictions and 1,800 corrective records remain unchanged; no predictions were generated. Eight hundred eighty-two external links were not refetched; original source citations remain historical.

Command: `C:/Python311/python.exe -X utf8 -B docs/plans/execution/research-consolidation-20261029-r01/receipts/check_docs.py`, through the owned subprocess runner. PID 13104, start `2026-10-29T21:32:40.069809+00:00`, end `2026-10-29T21:32:42.621867+00:00`, exit 0; internal 2.453042s, runner **2.547s/60s**. Stdout 2,946 bytes, stderr 0; output drained, normal process exit, no termination/cleanup needed. Script LF SHA-256 `d2db44723c7833bacd0dcf6ec0f87d35e98750a605b9519266591cb6b9a7b694`.

Initial pass used; **zero correction/rechecks**, no old prototype/checker or Rust execution. Subsequent status/receipt/review Markdown is manually inspected without repeating the preserved-data pass. Separate scoped PR2 review is the remaining acceptance gate before PR creation.

## Separate review disposition

**ACCEPTED**, no substantive findings or corrections. The [review](roles/reviewer/PR2_REVIEW.md) and [role record](roles/reviewer/PR2_ROLE_RECORD.md) preserve independent authorship, bounded read-only tree/scope corroboration and serving-model uncertainty. No checker or experiment was rerun. The actual stacked diff's ordinary `git diff --check <base>` also passed. The entire research/decision data remains frozen; only actual host-state/progress receipts follow.
