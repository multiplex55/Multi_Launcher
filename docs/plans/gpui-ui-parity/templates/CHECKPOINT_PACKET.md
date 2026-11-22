# Checkpoint packet: <goal ID / milestone ID>

**Template only — unfilled.** Status: **Pending planning**.

## Objective and evidence of need

- One concrete outcome:
- User requirement / GUI IDs:
- Verified current source and baseline commit:
- Existing behavior/invariants to preserve:
- Relevant recent fixes:
- Why this boundary is small enough for one implementer:

## Authority and ownership

- Existing task branch/checkpoint base:
- Planner packet prepared by:
- Single implementer:
- Read-only reviewer:
- Build/test owner and native controller:
- Current role configuration checked:
- Allowed source paths/symbols and core/frontend ownership:
- Explicit non-goals:

## Bounded implementation work

1. <Change with owning module and resulting behavior>.
2. <Integration/caller migration>.
3. <Removal of obsolete duplicated path if applicable>.
4. <Targeted verification and documentation update>.

- Required GPUI Kit API/version evidence:
- Shared-core contract and both frontend implications:
- UI-local state, event/thread/lifecycle rules:
- Persistence/backward-compatibility rules:
- Dependencies and genuine unresolved decisions:

## Verification packet

| Requirement / risk | Existing or required test | Exact package/target/filter or native steps | Expected result | Budget basis |
|---|---|---|---|---|
| <fill> | <fill> | <verified or explicitly pending discovery> | <fill> | <measured / bounded rationale> |

- Necessary compile configuration:
- Nonzero selected-case check:
- Fixture root and Paste Sequence hotkey opt-outs:
- Build versus test/native timing:
- Applicable existing evidence and invalidation scope:
- Native access prerequisite and independent work if unavailable:
- Concrete condition that would justify broader testing:

## Done and handoff

- Implementation behavior required:
- Evidence IDs that must pass:
- Independent-review acceptance and blocking-findings closure:
- GUI/status/decision documentation to update:
- Coherent local commit subject and path scope:
- Parent one best-effort push attempt:
- Next authorized checkpoint:

Implementer owns only this packet; parent coordinates later goals. Do not treat this template, a compile-only result or an unfilled evidence row as acceptance. Follow [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md).

