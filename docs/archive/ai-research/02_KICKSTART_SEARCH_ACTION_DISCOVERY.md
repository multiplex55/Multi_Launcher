# Kickstart 02 — Execute Intelligent Search and Action Discovery Research

Read [02_GOAL_SEARCH_ACTION_DISCOVERY.md](02_GOAL_SEARCH_ACTION_DISCOVERY.md) and [ORCHESTRATOR_GOAL.md](ORCHESTRATOR_GOAL.md) completely. Execute the second research goal after the audit handoff, not concurrently with it.

## 1. Reconcile the program and start on the right branch

Read PROGRAM_STATUS.md and Goal 01's mutable progress/SEARCH_HANDOFF. If audit work is still active, do not start a second research worker; resume the orchestrator's first goal until its handoff is closed. If the audit is explicitly partial, use the documented useful handoff and carry its limitations forward.

For new Goal 02 execution, create a fresh `codex/research-search-discovery-<RUN_ID>` branch from Goal 01's exact recorded closing documentation commit. Do not substitute a moving master, old integration branch, or an unrelated feature branch. The application baseline stays the program's pinned new master, with Goal 01 documentation inherited.

If this goal already has a legitimate branch/checkpoint, resume it without another baseline. Preserve all user changes and published research history. Resolve `GOAL_ROOT=docs/research/<PROGRAM_ID>/02-search-discovery`, establish M0-A and push normally.

## 2. Use the audit without repeating it

Read the existing source map, search handoff, applicable findings and future packages. Reinspect actual source only where needed to trace current query/ranking/activation owners or validate a load-bearing assumption.

Do not assume the application lacks ranking, synonyms, autocomplete or contextual actions. Identify what exists, its contract and shortcomings before proposing additions. Do not advertise unimplemented/deferred features in the proposed command catalog.

Keep planner, research implementer and reviewer sequential. Use the role/model recommendations and honest fallback policy. Every role returns or writes its substantive Markdown and complete future work proposals under its own role folder; the parent persists read-only role output and records attribution.

## 3. Execute source catalog, algorithm, and interaction design

Follow M1-A through M1-C in the goal:

1. Trace current query → routing → candidates → ranking → selection → explicit execution, including refresh tickets, usage data and plugin eligibility.
2. Build a source-backed catalog of real tools/actions, current syntax, safe opening behavior, target types, effects, capability limits and evidence.
3. Define intent families, aliases, ambiguity and protected literal syntax. Exact commands, paths, regex and pipelines must not be reinterpreted by an overbroad natural-language parser.
4. Compare deterministic ranking designs with explicit precedence, bounded usage influence, stable ties, availability rules and no-match behavior.
5. Design runtime ownership, caching/invalidation, schema evolution, opt-in migration/rollback and proposed resource budgets.
6. Specify keyboard-first result behavior and Universal Actions integration without introducing a second dispatcher or silently executing inferred operations.
7. Compare relevant official product documentation only where it helps decide a real design question; label unavailable external evidence rather than waiting for it.

Use detailed examples and counterexamples. Explain alternatives and failures, not just the chosen happy path. Do not add runtime LLM/cloud dependencies or workspace/scope-bar features.

## 4. Build a small offline prototype, not a test platform

Create a standard-library Python prototype under the goal's docs directory. It loads a synthetic/source-backed catalog and returns ranked IDs and explanations only. It must not execute commands, import live application modules, inspect user histories/clipboard, launch native applications, or call Cargo.

Generate a diverse synthetic relevance corpus, aiming at roughly 300 meaningful cases and at least 250 when justified by source scope. Include exact/literal compatibility, realistic discovery, ambiguous requests, typos, disabled/unsupported actions and dangerous-intent refusals. Document source links, labels and provenance; generated labels are not human ground truth.

Freeze family-level splits before final tuning. Keep held-out semantics separate where practical and disclose any author access to labels. A baseline that only approximates current policy must be called a surrogate, not measured launcher behavior.

Run only the bounded prototype work allowed by the goal: a small invariant check, at most three predeclared candidates, no large parameter search, 120 seconds per evaluation and 10 minutes aggregate. Measure appropriate relevance/safety/stability metrics with denominators and per-family errors. Do not turn Python timings into Rust performance claims.

If execution is unavailable, finish code/data/protocol, mark results NOT_RUN and continue the remaining design. Never invent numbers or stop the entire research project over an optional experiment.

## 5. Interpret results and generate complete implementation packages

At M2-B, select a design or document why the simple baseline is preferable. Preserve bad examples, no-gain outcomes, sensitivity and unresolved assumptions. Finish interaction/safety specifications and explicit future measurements.

At M3-A, create canonical future Goal/Kickstart pairs for every recommended change. Use real source owners and precise migration order, interfaces, invariants, commit boundaries, rollback and focused future tests. Include model choices appropriate to each package. Speculative ideas become bounded research spikes, not vague TODOs or immediately executable features.

Reconcile audit overlaps. Link shared-owner work to one canonical plan, preserve role authorship and dissent, and create `cross-goal/` amendments rather than rewriting Goal 01 history. Do not execute generated follow-ons.

## 6. Commit progress and close without another implementation queue

Update the goal's `Execution progress — mutable` section and program index at every meaningful transition. Commit each coherent milestone, inspect staged docs-only paths, and attempt one normal push. Failed push stays recorded and does not stop local research.

Run one substantive reviewer pass with focused corrections. Verify artifact links/IDs/schema and the final allowed-write scope. Do not run Rust builds, application tests, Clippy, native automation or benchmark campaigns.

Complete the program FOLLOW_ON_CATALOG, NEXT_IMPLEMENTATION_WAVE, HANDOFF, DECISIONS_AND_LIMITATIONS and documentation PR summary. Report actual source/branches/commits/pushes, research outcomes, prototype execution disposition, coverage/model limits and recommended next work.

Do not merge master, create production feature branches, or execute future kickstarts. A real session/permission/Git safety interruption must leave an exact Markdown resume point; ordinary unanswered research questions should become qualified conclusions or follow-on investigations while the rest of the program continues.
