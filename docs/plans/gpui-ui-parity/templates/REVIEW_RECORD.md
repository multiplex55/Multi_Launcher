# Independent review: <goal / checkpoint>

**Template only — unfilled.** Review status: **Pending**.

## Review scope

- Read-only reviewer role/configuration used:
- Checkpoint objective and acceptance IDs:
- Base commit and reviewed head/diff:
- Dirty changes included or excluded:
- Relevant implementation/source paths:
- Evidence records examined:
- Review boundaries and unresolved context:

Review actual code/diff against the current requirement and ownership contract. Do not rely on the implementer's summary alone. Respect the targeted verification budget; no unrelated audit or automatic full-suite run.

## Findings

| ID | Severity / blocking status | File or symbol | Concrete failure or missing requirement | Reproduction / reasoning | Required correction |
|---|---|---|---|---|---|
| <fill if present> | <fill> | <fill> | <fill> | <fill> | <fill> |

Check directly affected core behavior, frontend separation, state/event ownership, native effects, persistence, async lifecycle, egui fallback, recent fixes and test meaning. Distinguish a correctness issue from a style preference. “No findings” may be recorded only after review.

## Coverage and evidence

- Required behavior implemented versus missing:
- Evidence claims supported versus overstated:
- Native checks completed versus pending:
- Older evidence invalidated by this diff:
- Narrow additional verification needed, with reason:
- Scope additions proposed, if any, and necessity:

## Correction cycle and outcome

| Finding | Implementer correction/checkpoint | Focused recheck | Disposition |
|---|---|---|---|
| <pending> | <pending> | <pending> | <pending> |

- Final recommendation: Pending / Changes required / Ready for acceptance:
- Exact reviewed final source:
- Blocking findings remaining:
- Parent acceptance decision and linked evidence:

Reviewer remains read-only. Parent coordinates corrections with the single implementer; review approval does not substitute for missing mandatory evidence. Follow [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md).

