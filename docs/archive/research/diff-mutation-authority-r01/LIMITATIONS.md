# AUD-F002 M2-A — limits and unresolved guarantees

Date: 2026-10-29. Selected research outcome: **H0**, retain best-effort concurrent-writer authority as described in [DECISION](DECISION.md). M1-A evidence was committed/pushed as `ab5088f94f4f0d25783b05f4a2cdc3148d453270`; source/base is `59156d96adcc73939c7171487a0cb582e3286587`. Independent M2 review, formal artifact check and final publication receipt are pending at author handoff.

## Established facts versus unproven behavior

| Area | Established in this spike | Not established |
|---|---|---|
| Platform identity | Windows Diff compares kind/length/optional modification time; Unix also compares device/inode | Same-metadata prevalidation replacement on Unix with changed inode/device; any actual native replacement/incident |
| Effect authority | Diff validates then uses paths; no approved native source/destination guards retained through its effects | Exact approved-object/byte mutation, effect-time absence, or protected ancestor stability |
| Dirty state | Preview and dispatch collect dirty paths; worker checks its snapshot for destructive targets/descendants | An ongoing editor lock or external process exclusion |
| Directory operations | Copy selection is enumerated/normalized; directory creation has containment/path checks; files later run freshness checks | Atomic subtree snapshot, all per-file checks for directory creation, or per-descendant recycle identity |
| Destination read errors | `.ok()` in `fresh` collapses all identity-read errors to absence | A reproduced unsafe Windows effect resulting from a particular collapsed error |
| Outcome feedback | Executor produces per-item/partial/cancelled information; current GUI completion ignores item/cancelled fields | User-visible per-item feedback from this path, a specific missed user error, or a fix already implemented |
| Recycle admission | GUI mode is Recycle; worker rejects Permanent; application has no failure-to-permanent fallback | Unconditional Recycle Bin placement, approved native-object Shell targeting, reversible every-target behavior or native partial-effect accounting |
| Native reuse | Rename holds/revalidates source and directory guards and invokes no-replace rename by source handle, with absolute destination/null RootDirectory | A relative-parent-handle implementation, portable copy solution, or ready approved-overwrite/recycle primitive |
| Cancellation | Flag checked between items; prior effects remain; generation filters accepted reports | In-flight API cancellation, rollback, worker join or a complete list of unattempted items |

The exact sources, APIs and counterguards are in [CONTRACT_AND_EVIDENCE](CONTRACT_AND_EVIDENCE.md). The two source-proven information gaps support a [separate proposed follow-on](proposals/diff-failure-information/GOAL.md); the native uncertainties do not become stronger guarantees through that proposal.

## Backend and API limits

Trash 5.2.4's inspected Windows path uses parent canonicalization, parsing-name Shell item creation, IFileOperation deletion and `FOF_NO_UI | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING`. It checks abort after successful PerformOperations. This is stronger evidence than a crate-name assumption and weaker than an actual native targeting/recycling receipt. No claim is made about whether a constructed Shell item re-resolves later; the abstract swap is deliberately before construction.

COM/instance-creation panic sites and ignored completion-send failure mean normal-return reports are not a guaranteed terminal report for every worker exit. This source limitation is recorded, not expanded into a worker supervision project. The optional follow-on covers delivered current-generation reports only.

Official API documents were accessed on 2026-10-29. Current Rust docs identify 1.99.1, but the local compiler was not queried or run. File ID lifetime/support, normalized paths, access/share modes and Shell flags have OS/filesystem/driver limits. A native ID is not immutable content or a permanently unique historical identifier. Reusing an API name or a source comment is not native proof.

One API URL correction succeeded. Exact remote crate source retrieval and one alternate failed; the exact resolved local public registry source was inspected and hashed instead. Hashes identify inspected content, not independent package authentication. No further retrieval loop or unofficial substitute was used. [API_REFERENCES](API_REFERENCES.md) retains these limits.

## Evidence and verification budget

- Required identity proof is a hand-reviewed table with two substitutions and refusal counterexamples. **No executable model** was needed: zero invocations/states/time/failures. It proves only the logic of its declared assumptions.
- **No native operation, application fixture, real file mutation, user-data access, provider run, benchmark, dependency installation, Cargo/rustc/Nextest/Clippy or test execution occurred.** Nine existing test bodies were inventoried as source evidence only.
- One conditional future disposable native protocol is specified, with exactly two interleavings, expected native IDs/outcomes, explicit handle ownership and refusal of unexpected cleanup identities. It is **NOT_RUN** and not an executable implementation or authorization. Any later executor must first supply the stated safe cleanup mechanism; this spike does not claim that mechanism exists.
- Formal static artifact checks used by the author: **0**. Parent retains the shared one initial cheap pass after decision/review artifacts exist, plus at most one affected correction check. Manual source/reference/scope inspection and source hash authoring are not runtime tests.
- Configured author is `gpt-6-astra` / `xhigh`; actual serving backend/effective effort remains unverified. Independent review is not claimed until its separate receipt is persisted.

## Scope and decision limits

No production, configuration, test or dependency changes are included. No new native layer or stronger product contract is accepted. H0 documents current concurrency limits; it does not establish native safety by absence of an incident or declare that every user accepts those limits.

The generated failure-information Goal/Kickstart is **PROPOSED_NOT_AUTHORIZED**. It has distinct admission and GUI acceptance cases and leaves concurrent replacement/recycle semantics unchanged. Existing catalog execution authorization does not extend to it. No new catalog ID, schedule, implementation task, branch or execution has been created for it.

At author handoff, final task closure depended on the parent's separate review/check/publication work. The author did not fill in a speculative final commit SHA or claim those outstanding gates passed.

Parent closure, 2026-10-29: the [separate review](REVIEW.md) supported H0 and its one proposal-lifetime correction was applied. The initial static artifact check passed 190 assertions in 0.650496 seconds; zero correction rechecks. This confirms documentation/link/hash/scope consistency, not source semantics or native behavior. All native/product limits above remain. Actual closing commit/push is a subsequent parent queue receipt.
