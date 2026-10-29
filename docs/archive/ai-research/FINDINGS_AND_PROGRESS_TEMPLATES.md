# Findings, Decisions, Role Provenance, and Progress Templates

These templates are instantiated by the executing research roles and orchestrator. They are not claims that research has already occurred. Fill them with actual source/evidence; explicitly mark unavailable values instead of inventing them. Generated final artifacts should not leave placeholder text for the user to supply.

## 1. Finding record

Use one document per material finding when detail warrants it. Keep smaller no-action observations in the finding index.

```markdown
# AUD-001 — Specific finding title

State: Observed / Static inference / Hypothesis / Historical / Not applicable
Category: Correctness / Lifecycle / Input / Persistence / Privacy / Change cost / UX
Source baseline:
Author role/model:
Review provenance:

## Current behavior and owner
Describe actual entry points, state owners, callers and the relevant branch.
Cite pinned repository paths, symbols and exact source ranges where available.

## Evidence
- Primary source claim and link.
- Relevant caller or guard that confirms/limits it.
- Historical evidence with its original tree, if used.

## Mechanism and user impact
Explain the precise failure sequence or maintenance cost.
Separate what the source establishes from assumptions about scheduling/runtime.

## Counterevidence and limits
What existing guard may prevent it? Under what conditions is it harmless?
What has not been inspected or measured?

## Classification
Severity:
Confidence:
Likelihood/conditions:
User value of action:
Implementation breadth:
Validation burden:

## Alternatives and recommendation
No change / existing-owner fix / narrow extraction / wider redesign.
Explain the selected action and why smaller alternatives do or do not suffice.

## Current and future proof
Current: static only / historical receipt / actual isolated prototype.
Future: narrow evidence needed; no claim it ran during research.

## Disposition
Ready for implementation review / Needs research spike / Already covered /
Duplicate / No action.
Canonical follow-on GOAL and KICKSTART links, or exact disposition rationale.
```

Do not use the word “confirmed” for a race or performance outcome unless the evidence supports that precise assertion. A direct source contract inconsistency can be confirmed statically; a possible interleaving may still require runtime evidence.

## 2. Decision record

```markdown
# DEC-### — Specific decision

Question:
Source/requirement context:
Decision status: Proposed / Selected for prototype / Selected for future implementation /
Rejected / Conditional / Unresolved

## Alternatives
Describe the relevant options, including no change and an existing-owner repair.

## Evidence and criteria
Correctness, compatibility, UX, complexity, runtime cost, privacy, migration.
Mark proposed budgets versus measurements.

## Decision and rationale
State the choice, its tradeoffs and why it is proportionate.

## Counterargument and reversal trigger
What evidence would cause a different choice?

## Consequences
Affected owners and interfaces; dependencies; future tests and follow-on package.
```

A decision record should preserve engineering reasoning the future implementer needs, not hidden chain-of-thought transcripts. Provide concise, inspectable arguments, calculations, assumptions and counterexamples.

## 3. Role record

```markdown
# Planner — Role record

Program / goal / checkpoint:
Requested model / effort:
Observed resolved model / effort: value or unverified
Execution mode: separate child / parent fallback
Role/thread ID: value when observable
Authored by:
Persisted by:
Source baseline:
Inputs read:
Outputs produced:
Review independence:
Known limitations:

## Contributions
Checkpoint/date → analysis, decision, review or follow-on paths.

## Follow-on proposals
Candidate ID → complete pair or duplicate/no-action disposition.
```

Use equivalent records for implementer, reviewer and orchestrator. A read-only role may author content that the parent writes to disk; record that accurately. A parent-authored fallback does not become an independent reviewer merely because it is saved in the reviewer folder.

## 4. Canonical recommendation index

For each goal, and then once across the program:

| ID | Recommendation | Evidence/finding | Readiness | Priority/confidence | Owner | Dependencies | Models | Goal | Kickstart |
|---|---|---|---|---|---|---|---|---|---|
| Actual ID | Concrete action | Link | Actual disposition | Explain | Actual owner | IDs/capabilities | Role setup | Link | Link |

Add estimated complexity/regression risk/validation burden as qualitative fields if helpful. Do not invent time savings or hour estimates. Use exact links for Already-covered/Duplicate entries. Speculative recommendations point to a complete research-spike pair.

At final handoff, reconcile three directions:

1. Every actionable finding has a recommendation/disposition.
2. Every accepted recommendation has a complete canonical pair or explicit existing-plan delta.
3. Every package maps to evidence and an authored role contribution.

A failed benchmark candidate is not automatically a new work item. Explain whether it is rejected, needs a spike, or identifies an actual implementation opportunity.

## 5. Per-goal progress

Update the `Execution progress — mutable` section in the main goal file, not a disconnected local-only note.

```markdown
State: IN_PROGRESS / RESEARCH_COMPLETE / RESEARCH_COMPLETE_WITH_LIMITATIONS /
PARTIAL / PAUSED_RESUMABLE / BLOCKED_SAFETY
Program ID:
Source SHA:
Actual branch/worktree:
Active checkpoint and writer:
Last known committed checkpoint/push:
Exact next action:

| Checkpoint | State | Real artifact links | Evidence/checks | Known commit/push | Limit/next action |
|---|---|---|---|---|---|

## Event log
UTC timestamp — substantive transition, decision, accepted output or limitation.
```

Do not silently lower the charter's requirements to make a status green. Add a dated amendment for a material scope change. Record the preceding known commit, not an impossible self-referential resulting SHA.

## 6. Program status

`docs/research/<PROGRAM_ID>/PROGRAM_STATUS.md` records orchestration, not duplicate detailed goal findings.

```markdown
# Sequential Research Program — Status

Program ID / source / plan root / output root:
MASTER_START and intended remote:
Current goal/checkpoint / active writer:
Requested/actual role configuration:
Current branch/worktree:
Last known local/remote commit and actual push disposition:
Next action:

| Goal | Research outcome | Branch | Base | Closing commit | Handoff | Limits |
|---|---|---|---|---|---|---|

## Branch/capability reconciliation
No production changes; latest source drift; child ownership and fallbacks.

## Artifact indexes
Goal01 README; Goal02 README; canonical follow-on catalog; decisions; stalls.

## Material events
Dated handoffs, resumed checkpoints, model fallbacks, actual push results.
```

The source baseline file records the immutable research snapshot separately. The program status and goal progress must agree, but they should not become two competing command ledgers.

## 7. Genuine stopping report

Use `OUTPUT_ROOT/stalls/<goal>-<checkpoint>-<UTC>.md` only for an actual stop or pause, not every unavailable optional resource.

```markdown
# Goal/checkpoint — Paused or blocked report

Time and state:
Program and source baseline:
Actual branch / worktree / known HEAD / upstream:
Last accepted research checkpoint:

## What was being done
Exact question, artifact or command; no vague “stalled” description.

## What happened
Observed error or unavailable capability; expected versus actual.
If a command ran, include exit/deadline/output/cleanup facts that are known.
Do not invent process receipts for static-only work.

## Work preserved
Files, uncommitted/staged state, completed role outputs, pending push.
Any active owned writer/Git operation and how it was reconciled.

## Recovery attempted
One bounded fallback, result, and remaining authorization.

## Continue vs defer
Which unrelated research can proceed? What specifically cannot?
Why an uncertainty is not being misrepresented as resolved.

## Exact next action
Resume path, branch, checkpoint and first executable step.
User decision needed only for a genuine unresolved authority/safety issue.

## Publication
Local report path, commit if already known, actual push result or unpublished.
```

Do not discard user work to publish a report. If an unsafe index/merge prevents a commit, preserve the report in an owned safe location and state it is unpublished. Write the tracked version when safe, without a force operation.

## 8. Reviewer checklist

The reviewer should answer substantive questions:

- Do source citations actually support each load-bearing claim?
- Are actual behavior, inferred behavior and proposal separated?
- Were relevant counterguards inspected before calling something a defect?
- Are observed prior-queue failures distinguished from excluded/unmerged candidates?
- Does a design reuse existing owners rather than duplicate systems?
- Are future APIs/state transitions/compatibility explicit enough to implement?
- Is the benchmark synthetic provenance clear, with no mislabeled runtime performance?
- Are split leakage, multiple acceptable answers, unsupported queries and hard safety rules addressed?
- Does every actionable recommendation have a complete follow-on pair?
- Are speculative items research spikes rather than invented production fixes?
- Are role/model identity and review independence truthful?
- Are no-build scope, branch ancestry, progress records and no-auto-follow-on boundaries intact?

One correction pass should fix factual/content defects. Optional style suggestions do not justify an endless rewrite cycle.
