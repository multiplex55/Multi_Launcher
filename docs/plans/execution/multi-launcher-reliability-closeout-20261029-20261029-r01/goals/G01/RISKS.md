# Risks

Preserve H0 best-effort mutation contract; destination observation errors must fail before effect. GUI report must bind generation, view and ordered roots. No global framework or unrelated refactor.

## M1-A scope and proof limits

- Only non-NotFound destination observation errors change behavior: they produce an informative per-item validation failure before `fs::copy`. Expected destination comparison, source/root identity, captured paths, containment, dirty protection, direction and cancellation remain in the existing execution path.
- This preserves H0 best-effort path/metadata behavior. It does not add native object binding, atomicity, rollback or race-free publication guarantees.
- Compilation and execution are `PENDING` for the approved combined G01 batch after M1-B. Source inspection and authored tests are not a passing test receipt.
- M1-B GUI feedback work is outside this checkpoint and was not started.
- Independent new findings: `NO_ACTION`. The observed issue is the assigned destination-error defect; no additional actionable finding justified a new follow-on package.

## M1-B scope and proof limits

- Feedback retains one delivered outcome, accurate completed/failed counts, cancellation and at most 20 exact path/reason details with an omitted count. It is transient runtime state, not persisted comparison state. Successful preceding items are counted without rollback or unattempted-work claims.
- Generation, view and ordered roots must agree before adoption. Same-comparison rescan clears scan resources while retaining feedback; new dispatch replaces it; discarded/changed comparisons and explicit swaps clear it, including a retained folder parent.
- At the M1-B checkpoint, preparation of a new scan cancelled/dropped the stale operation handle. Independent review identified this as G01-R1 because the current item can still mutate; the correction and remaining delivery limits are recorded below.
- Tests and compilation remain pending for M2-A. No native identity, trash, race-free publication or GUI pixel-precision proof is claimed.
- Independent new findings: `NO_ACTION`; the retained-parent and preview lifetime corrections are within the approved G01 packet. No new follow-on pair created.

## M2-A observed limitation and disposition

- The 30-case focused selection now passes on the committed M1-A/M1-B source, including all 17 new tests and the unchanged seven Diff domain cases through a temporary target. Earlier pending statements are historical checkpoint status. Independent review/acceptance remains the parent's responsibility.
- The original `domain` target fails to compile because the untouched `file_search_e2e::filenames` fixture passes the former raw sender type. This is an actionable independent test-caller migration finding, not a G01 product-code failure. Its complete proposed [Goal](roles/implementer/follow-on/file-search-sender-fixture/GOAL.md) and [Kickstart](roles/implementer/follow-on/file-search-sender-fixture/KICKSTART.md) preserve the exact first diagnostic, current owner, proof limits and bounded future verification. Parent deduplicates and activates it; no File Search edit occurred.
- The authorized wrapper recovery proves the selected unchanged Diff cases, not the monolithic domain target. The wrapper and task processes are gone, and original fixtures/configuration are unchanged. No budget cap was reached or silently renewed.
- Existing H0, native identity, mutation race, worker retirement and recycle-delivery limitations remain unchanged. Passing deterministic tests do not widen those guarantees.

## G01-R1 correction limits

- Same-comparison rescan now retains the active handle/busy gate until its matching terminal event. A stale matching terminal only acknowledges completion; it cannot adopt feedback or rescan. Wrong context/generation cannot release another operation. The two new owned-channel tests and original selection pass (32/32).
- This preserves the existing exclusion gate without joining workers or changing delivery. If an operation never sends a terminal event, its gate remains held for that comparison; channel-disconnect recovery and worker-retirement redesign are outside this correction. Explicit comparison invalidation still cancels/discards its runtime as before.
- No independent new finding was discovered during this correction. Reviewer-owned G01-R1 contingency status remains for parent disposition after named reviewer confirmation; it was not duplicated or scheduled. The separate File Search fixture pair remains proposed and unexecuted.
