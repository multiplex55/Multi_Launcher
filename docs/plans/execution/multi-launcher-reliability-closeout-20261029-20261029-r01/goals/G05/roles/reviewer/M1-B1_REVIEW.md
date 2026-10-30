# G05 M1-B1 independent review

Context /root/reviewer, requested Astra/max, configured Sol/high unchanged, actual backend UNVERIFIED. Branch codex/closeout-g05-hotfolder-condition-evaluator-20261029-r01; checkpoint bab309689e5acf0b1924fb20842fe06c53b37fec; diff base1c5b3b88acc2c285a08b00d3aeb6406bbe08c045. Parent persists returned content.

Read amended contract/handoff/roles/source/risk/verification receipts, complete scanner/composition diff and seven new tests. No edits, Git mutations, Cargo, native calls, dispatch or B2 design.

## R1 — P2 terminal byte accounting

LiteralScan::finish conflates supplied-but-unprocessed bytes with missing source bytes. feed can return ContentChunk, Cancelled or Deadline before updating observed_bytes. Genuine EOF with unchanged final length then replaces Unavailable with Changed.

Counterexample: expected length MAX_CHUNK_BYTES+1, supply that same valid chunk; feed returns ContentChunk before counting; finish Eof with same final length/Current returns Changed. No source change. Existing oversized test finalizes ReadFailed, missing this. Execution still refused, but typed classification violates contract.

Correct scanner accounting/finalization, preserving sticky failure and real Changed precedence. Test EOF after resource rejection and entry-time cancel/deadline before final chunk, plus genuine length-change/premature-EOF controls, both modes. One bounded correction required; B1 acceptance withheld.

## Other evidence and limits

First focused20/20 does not cover R1. All13 receipt blobs match checkpoint; cumulative diff--check passes,31 local links resolve, both owned processes exited/drained. No other substantive findings: bounds/UTF8/KMP/exact-set private evidence/explicit completion/sanitized Debug coherent; metadata/common composition/gates retained, native/execution/GUI/persistence unchanged. M1-A unaffected. Native acquisition/retirement/source effect/actual queued start still B2.

Complete contingency pair in follow-on/terminal-byte-accounting; mark RESOLVED_IN_G05 if named correction accepted, do not schedule separately then.
## G05 M1-B1 R1 confirmation — 2026-10-30

Independent /root/reviewer; requested Astra/max, configured Sol/high unchanged, actual backend UNVERIFIED. Checkpoint944f9d0ef4ed5ceee54c9a3d255c40e9a220494d; correction base6cbb6e43f8655b62e3cd29b836a24ba9335ef0b4. Named R1 only, no fresh broad review, edits, Git calls, Cargo/native/dispatch/B2 work. Parent persists returned content.

R1 RESOLVED. received_bytes counts supplied chunks before parser/cancel/deadline/chunk-limit checks, including after refusal. Unchanged-length EOF keeps Unavailable for oversize/entry cancel/deadline. Actual overrun strengthens prior Unavailable to Changed; true prematureEOF/final-length/source-binding change retains Changed. Sticky failure prevents completed evidence/Match. Saturation cannot hide overrun against <=8MiB admission.

Two new both-mode tables assert exact outcomes/no completed evidence. Existing test separately retains empty-feed sticky failure and recognizes real extra-byte overrun. Reviewed correction/receipts/full22/22 output (old20+2), no ignored/retries;95.941s/600 preparation,3.909s/180 run. Independently computed conditions0559e8f9cc77430b210ab7949967ab8582cd6c99 matches receipt. Both owned processes exited/drained.

ACCEPTED_COMPLETE for corrected cumulative B1 ONLY. Initially rejected standalonebab stays historical, never independently accepted. M1-A unaffected, all condition gates closed, native/B2/editor/TestRule outside this acceptance. terminal-byte-accounting RESOLVED_IN_G05, no future queue entry. NO_ACTION further findings. Parent reconciliation/cumulative provenance import.
