# G09 M1 independent review

2026-10-30. **ACCEPT M1 test-strengthening and its bounded evidence; selected production logic NO_CHANGE_REVALIDATED.** No substantive finding or required source correction. Reviewed `691645e357d94f240a544996f5a58b532955a345..090df0d0859189d63804c4e79524fb7a3be2478b` on `codex/closeout-g09-activity-center-retirement-20261029-r01`, independent bootstrap `02d0ac64`. Parent reported the checkpoint pushed and owns final milestone/ledger reconciliation and integration. This acceptance covers the two selected owner protocols and the specified Duplicate Finder native handle case, not a global retirement guarantee.

Requested reviewer Astra/max; configured `.codex/agents/reviewer.toml` Sol/high, unchanged; actual serving backend/model/effort **UNVERIFIED**. Reused independent-review context but assessed only current G09. Read the complete active Goal/Kickstart, [owner proof contract](../planner/OWNER_PROOF_CONTRACT.md), [M1 handoff](../planner/M1_HANDOFF.md), [implementer record](../implementer/ROLE_RECORD.md), [verification](../../VERIFICATION.md), exact diff and directly relevant current owners/tests. Reviewer performed read-only source/Git/receipt/link checks and authored only this Markdown. No Cargo, test rerun, native probe, source edit, Git mutation or child dispatch.

## Findings and acceptance rationale

**NO_ACTION — no severity-ranked substantive finding.** The changed source files are only `src/thread_reaper.rs`, `src/duplicate_finder/tests.rs` and `src/gui/screenshot_capture.rs`. All edits are test-only; independent comparisons of both inline-test files' non-test prefixes against the baseline are equal, and the Duplicate Finder tests module is gated by `cfg(test)`. No production owner, manifest, feature, default, polling policy or public interface changed.

- **Reaper:** the held worker deliberately drops its notifier before exit. While held, the callback is absent and the real reaped probe is false. After release, the callback reports a successful join outside the reaper lock. The probe is set by `supervise` only after `JoinHandle::join`. The injected supervisor-spawn failure drops the callback, retains capacity until notifier completion and never reports a successful join, including after capacity returns. These assertions distinguish notification, capacity reclamation and actual join.
- **Duplicate Finder:** the real session retains admission while a cancelled scan drains. Refused start/reopen attempts preserve generation and call count; close invalidates old results. `finish_close` requires successful `reopen`, which rejects pending, panicked and failed-reaper states, and the test separately requires the original worker's actual reaped probe. Reopened state contains no old roots; a fresh generation completes with its own root before its own close/join. The cancel-only case also preserves the Cancelled snapshot after the old work returns.
- **Screenshot:** direct cancel retains Draining/busy and refuses a second schedule without changing generation or worker count. Owner `tick` actually joins before confirming the exact Activity row and returns no stale image. Shutdown instead transfers the worker, permanently refuses later scheduling and leaves the reaped probe false while held. Its terminal Activity row truthfully remains `Unavailable` after the separate reaper probe proves join; no nonexistent Activity callback is inferred.
- **Activity/UI:** selected controls exercise exact cancellation capability, bounded terminal history and late-retirement-after-clear fencing, safe copied metadata, and the visible/open/250 ms sampler gate. Source inspection corroborates unchanged live-operation preservation and omission accounting. No UI state was made an owner-admission or retirement authority.

The strengthened blocked fixtures install release-on-unwind guards before the controlled owners and use bounded waits for actual owner/probe predicates. Passing tests explicitly release and join their work; supervisor failure deliberately retains detached/no-join semantics. No elapsed-time threshold or terminal row substitutes for a positive join assertion.

## Native claim and its limits

The [native receipt](../implementer/verification/native-handle.json) matches the passing test's emitted evidence and actual assertions. Eight synthetic 128 KiB files first permit exclusive read opens. The existing production `open_duplicate_discovery_content_file`/Windows leaf acquisition owns the native file before `FirstReadCheckpointGate` blocks; `NativeContentReader::read` places that gate **before** `self.file.read`. The production content handle permits read/write/delete sharing, so successful deletion would be an inadequate observation; the test instead uses an exclusive `share_mode(0)` read probe.

Fixture index 0 specifically refuses with Windows error 32 while the source is held, including after cancel/close; unrelated errors fail the test. Reopen remains refused and the probe remains false. Only after release and successful owner join/reopen do all eight exclusive opens succeed. A warm scan then returns one duplicate group, its worker is separately joined, and explicit fixture removal succeeds. This provides the selected acquire/hold/release evidence through the existing native reader and RAII path.

Accept that exact native handle case only. In-flight `ReadFile` interruption, arbitrary blocked-I/O cancellation, native screenshot acquisition, COM/apartment cleanup, worst-case latency and other owners are **NOT_RUN/unclaimed**. Screenshot's injected images provide lifecycle evidence, not native capture proof. These limits are explicit in the existing owner matrix and are not new follow-on findings.

## Verification and identities

The exact non-ignored inventory contains 12 library tests. Recorded serial Nextest run: **12/12 PASS**, exit 0, 3.648 seconds wall / 180-second cap, 0.732 seconds test execution, no retry. Preparation took 211.037 seconds / 1200-second cap without timeout; its numeric exit remains **null/unavailable**. Complete nonzero JSON, compiler completion and the matching-source successful run support the proof; this review does not invent an exit code or require a redundant preparation. Incremental preparation used 0/600 seconds.

All ten source/manifest blobs independently match the [run receipt](../implementer/verification/initial-run.receipt.json) at `090df0d0`. The three changed blobs are:

| Test source | Blob |
| --- | --- |
| `src/thread_reaper.rs` | `ab1efa9dc88b3efd87ba7d9986fc052a29acf128` |
| `src/duplicate_finder/tests.rs` | `18788605f3024df573c89c8703c520e4cf3c903d` |
| `src/gui/screenshot_capture.rs` | `168cb728eedc115f001880e15a9fd5d6d7f30172` |

The cumulative authored Rust/Markdown whitespace check passes. All 24 existing relative Markdown links in the G09 packet resolved before this record was added. The raw run log's final blank line is preserved output, not an authored-source defect. [Cleanup](../implementer/verification/cleanup.json) records drained sessions, no remaining recorded or build/test processes and an empty owned fixture root. I inspected that receipt, not a fresh native/process probe. No full-app, full-suite, domain, native UI or clipboard claim is made.

## Disposition

The exact three test-source changes and supporting G09 documentation/evidence are eligible for parent integration. Selected existing production behavior is **NO_CHANGE_REVALIDATED**; a production fix is neither demonstrated nor needed. Parent may close the selected native case and owner matrix with the limits above, reconcile mutable checkpoint fields, and record actual integration/commit/push outcomes. No broad retirement acceptance follows from this recommendation.

Distinct unresolved recommendation: **NO_ACTION**. No new Goal/Kickstart pair is warranted, and no additional verification or correction loop is requested.
