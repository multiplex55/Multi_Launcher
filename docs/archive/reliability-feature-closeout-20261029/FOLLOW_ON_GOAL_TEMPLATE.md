# FOLLOW-ON GOAL — <stable-id> — <concrete owner/change>

State: `PROPOSED_NOT_AUTHORIZED` or `NEEDS_RESEARCH_SPIKE` (never assume authorization). Originating Goal/role/model, actual source SHA, source evidence and counterevidence, reviewer disposition.

## Outcome and user value

Specify actual negative behavior or high-value proposed behavior. Distinguish observed source, runtime reproduced, speculative and design-only. Explain why existing owners do not already satisfy it. No invented frequency or performance claim.

## Scope, current owner, exact callers

Provide real paths/symbols and input/state/output flow, source of truth, permission/failclosed/activation/async ownership. Revalidate on then-current master. Name adjacent owners not to touch and why.

## Proposed behavior and migration

Define selected algorithm/state transitions, type/field/identity change, compatibility with saved data/defaults, UI behavior, cancellation/shutdown/retirement, privacy, rollback, error propagation and failure policy. Contrast smaller and no-change options; select one with confidence and reversal condition.

## Milestones and commits

M0-A safe baseline/branch/ledger; M1-A minimal source owner; M1-B UI/client integration if necessary; M2-A focused meaningful tests; M3-A separate review and handoff. For each: dependencies, exact changes, named tests, done criteria, one checkpoint commit/push.

## Required future validation

Name owned fixtures, positive/negative tests, actual command/selected test inventory and need (or explicit lack of need) for native proof. Time/outcome/cleanup budgets. List high-risk regression cases, not an endless matrix.

## Dependency/authorization/decision gate

Specify branch base from then-current verified master/explicit integrated accepted dependency, user authorization requirements, stopping conditions, and whether this is implementation-ready or only a bounded research spike. No auto execution, no automatic merge.

## Mutable progress

Status NOT_STARTED; source/base/branch unverified; no executed tests, commits or merges from generating this file. Exact next step after explicit selection.
