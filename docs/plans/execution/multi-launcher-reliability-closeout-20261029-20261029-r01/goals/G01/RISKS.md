# Risks

Preserve H0 best-effort mutation contract; destination observation errors must fail before effect. GUI report must bind generation, view and ordered roots. No global framework or unrelated refactor.

## M1-A scope and proof limits

- Only non-NotFound destination observation errors change behavior: they produce an informative per-item validation failure before `fs::copy`. Expected destination comparison, source/root identity, captured paths, containment, dirty protection, direction and cancellation remain in the existing execution path.
- This preserves H0 best-effort path/metadata behavior. It does not add native object binding, atomicity, rollback or race-free publication guarantees.
- Compilation and execution are `PENDING` for the approved combined G01 batch after M1-B. Source inspection and authored tests are not a passing test receipt.
- M1-B GUI feedback work is outside this checkpoint and was not started.
- Independent new findings: `NO_ACTION`. The observed issue is the assigned destination-error defect; no additional actionable finding justified a new follow-on package.
