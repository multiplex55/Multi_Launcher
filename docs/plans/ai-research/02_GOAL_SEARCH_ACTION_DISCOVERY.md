# Goal 02 — Intelligent Search and Action Discovery: Architecture, Algorithms, and Implementation Library

**Program:** Multi Launcher Sequential Research Program  
**Repository:** `multiplex55/Multi_Launcher`  
**Goal type:** Source-grounded product/algorithm research, detailed architecture, small offline prototype, future-work packages  
**Companion:** [02_KICKSTART_SEARCH_ACTION_DISCOVERY.md](02_KICKSTART_SEARCH_ACTION_DISCOVERY.md)  
**Governing contract:** [ORCHESTRATOR_GOAL.md](ORCHESTRATOR_GOAL.md)  
**Position:** Second; begin only after Goal 01's research handoff is closed.

## A. Product objective

Design a much better way to discover Multi Launcher's existing tools and actions without memorizing every exact prefix. Preserve deliberate commands and literal payloads. Improve useful recommendations through a deterministic, local-first catalog and ranking layer rather than adding an LLM to every keystroke.

Example *proposed* discovery experiences include “compare two directories,” “calculate a file checksum,” “make JSON readable,” and “show background work.” The resulting suggestion should open an existing appropriate tool or expose an eligible action. It must not claim an unimplemented capability, guess a destructive operation, or turn data text into an executable shell command.

This is not a production search rewrite. Produce the current architecture, proposed design, algorithm choices, examples, evaluation assets, compatibility constraints, and detailed follow-on Goal/Kickstart pairs. Any prototype runs in isolation under docs and never invokes the launcher, plugins, native OS actions, or user data.

### Primary design outcomes

- Discoverability improves without breaking exact prefixes, calculator expressions, paths, URLs, regex text, clipboard transformations, or other explicit syntax.
- Results are stable, comprehensible, keyboard-friendly, and eligible under actual plugin/settings/context capabilities.
- Existing search, typed commands, Universal Actions, ranking, and usage owners are reused where appropriate.
- Startup, idle work, memory, and query-time cost have reasoned budgets and a future measurement plan, not invented performance claims.
- All recommended production changes are packaged as separately authorized future work.

## B. Baseline and dependencies

Use the same pinned application source as Goal 01. Under the program, create a fresh Goal 02 branch from Goal 01's exact closing documentation commit; the source still corresponds to `MASTER_START`. Read `SEARCH_HANDOFF.md`, source map, relevant findings, coverage limits, and canonical follow-on index.

Do not redo the entire static audit. Verify only owners relevant to this goal and any statement that becomes load-bearing. If the audit handoff is partial, mark the missing input and investigate that narrow area here; unrelated audit uncertainty must not block useful search design.

Historical navigation seeds, all to be revalidated:

```text
src/plugin.rs                     # plugin API, routing, capabilities, refresh tickets
src/commands/                     # typed commands, parsing, activation sources
src/universal_actions/            # targets, providers, eligibility and ordering
src/common/query.rs               # query policy and existing filters
src/usage.rs and src/history.rs    # actual persisted usage/history semantics
src/completion.rs                 # current completion behavior
src/gui/                         # query field, result selection, dialogs, focus, Jump
src/settings/ and src/settings_editor/
src/plugins/ and src/plugins_builtin.rs
```

The prior source showed both `ActionFilters` and `Literal` plugin query policies, typed command invocation, Universal Action providers, and usage persistence. These are discovery hints, not a claim that master still uses the exact same interfaces. Trace the real current implementation and baseline ranking before proposing a replacement.

Explicit non-goals: new workspace features; restoration of the removed All/Apps/Files/Notes/Commands scope bar; runtime cloud calls; a runtime LLM; silent clipboard/file sampling; executing inferred commands; broad plugin API replacement; implementing deferred utilities from the old queue; compiling Rust or validating native input now.

## C. Required deliverable set

Resolve `GOAL_ROOT = docs/research/<PROGRAM_ID>/02-search-discovery`.

| Artifact | Required contents |
|---|---|
| `README.md` | User outcome, design summary, proposed vs existing behavior, reading order |
| `CURRENT_SEARCH_ARCHITECTURE.md` | Real query-to-result-to-command flow and existing ranking/usage semantics |
| `COMMAND_AND_CAPABILITY_CATALOG.md` | Human-readable command inventory, source-backed eligibility and partial capability limits |
| `catalog/catalog.json` | Structured prototype catalog, stable synthetic IDs mapped to real command/tool owners |
| `INTENT_TAXONOMY_AND_GRAMMAR.md` | Intent families, aliases, ambiguity, protected syntax, and no-match policies |
| `RANKING_SPECIFICATION.md` | Scoring/precedence, deterministic ties, normalization, personalization bounds and examples |
| `RUNTIME_AND_INTEGRATION_DESIGN.md` | Proposed types/interfaces/data flow, caches, revisions, invalidation, budgets and migration |
| `KEYBOARD_AND_RESULTS_UX.md` | Result presentation, focus/selection, context actions, no-result behavior and interaction matrix |
| `PRIVACY_SAFETY_AND_COMPATIBILITY.md` | Threat/abuse cases, payload boundaries, action authority, backward compatibility |
| `ALTERNATIVES_AND_EXTERNAL_REFERENCES.md` | Compared approaches and relevant official product/technical sources |
| `DECISION_RECORDS.md` | Important choices, counterarguments, evidence, reversal triggers |
| `evaluation/DATASET_README.md` | Synthetic labeling methodology, families, splits, limitations and provenance |
| `evaluation/queries.jsonl` | Source-grounded labeled queries and contexts; no private user history |
| `evaluation/EVALUATION_PROTOCOL.md` | Baselines, split rules, metrics, predeclared comparisons and limits |
| `prototype/README.md` | Reproduction, dependencies, exact scope, run limits, no-Rust/no-OS-action warning |
| `prototype/*.py` | Small standard-library prototype, not a production subsystem |
| `evaluation/RESULTS.md` | Real results or explicit NOT_RUN; errors, no-gain outcomes, per-family analysis |
| `EVALUATED_EXAMPLES.md` | Walk-throughs of clear, ambiguous, protected, unavailable, and unsafe inputs |
| `RECOMMENDATIONS.md` | Implementation recommendation, confidence, value and dependencies |
| `FOLLOW_ON_INDEX.md` | Full canonical future work mapping |
| `follow-on/SEARCH-F###-<slug>/` | Complete future GOAL.md and KICKSTART.md with detailed acceptance |
| `REVIEW_AND_LIMITATIONS.md` | Independent/same-parent review label, open concerns and corrections |
| `roles/` | Actual role/model contributions and their actionable proposed work packages |
| `cross-goal/` | Reconciliation of audit findings and search recommendations without rewriting Goal 01 |

Markdown is the primary human deliverable. Structured files exist to make the catalog, examples, and prototype reproducible. Do not leave empty placeholder “results” or imply code was run when it was not.

## D. Current-system discovery before design

### D1. Separate routing, candidate generation, ranking, and execution

Trace the actual pipeline and label each owner:

```text
Raw query
→ syntax/prefix/query policy
→ provider eligibility and dispatch
→ candidates and refresh generations
→ filtering/ranking/usage adjustments
→ visible result ordering and selection
→ explicit user activation
→ typed command validation and execution
```

Determine whether some steps are interleaved or duplicated. Inspect caller code, not just the plugin trait. Find ranking across normal actions and plugin results; identify fuzzy matching implementation, score normalization, exact/prefix handling, stable tie rules, and usage contributions if present. Record absence only after a targeted search.

Read persisted usage/history types and actual updates. Do not promise recency/time-of-day personalization if the current data only stores counts. Propose new fields only with migration/privacy justification and a fallback for users with no history.

Trace dynamic plugins and enabled-capability rules. Determine whether a discovery catalog can be generated from existing metadata, needs a minimal extension, or requires curated annotations. Do not treat all plugin `search` implementations as safe to call for catalog construction or every keystroke; some may initiate work.

### D2. Build an actual command/capability catalog

For each meaningful discoverable command/tool, record:

- Stable identity or proposed stable identity with real owner mapping.
- Existing label, description, exact prefixes/aliases and payload/query policy.
- User goal and safe entry action.
- Supported target types and whether arguments can be prefilled safely.
- Enablement, capability, current-context and availability requirements.
- Effects: pure display, local read, clipboard write, file mutation, external launch, unattended action, or destructive operation.
- Confirmation/preview requirements and the existing owner enforcing them.
- Partial/deferred status: which interfaces are actually usable in current master.
- Source evidence and an example supported invocation.

Do not add aspirational actions as available catalog entries. For instance, a ZIP planning core without an extraction UI is not an “Extract ZIP” command. A macro summary collector without a history UI is not a “Show run history” action. A deferred full-file export must not appear as ready merely because the code contains a table module.

A curated discoverability entry can safely open an existing tool, but any payload transformation or prefill requires a typed mapping and validation contract. Label proposed catalog additions distinctly from current registration.

### D3. Preserve exact, explicit behavior

Inventory protected forms in the actual source: command prefixes, query filters, literal plugin payloads, calculator expressions, filesystem paths, URLs, quoted arguments, regex, macro syntax, clipboard pipelines, and saved action identities.

For each form specify:

1. How it is recognized using existing parsing/ownership.
2. Whether discovery is bypassed, secondary, or permitted.
3. How raw payload bytes/text are preserved.
4. Which owner validates execution.
5. A regression example that would detect reinterpretation.

Do not arbitrarily tokenize/remove punctuation from a regex or path. Do not consume a word inside a literal payload as a global filter. Preserve current explicit command precedence before adding fuzzy intent suggestions.

## E. Intent and grammar design

Design an intent taxonomy that matches the current application rather than an abstract universal assistant. Useful conceptual families include finding/opening, comparing, transforming text/data, computing, inspecting activity, capturing/annotating, configuring, and automation. Validate each family against real capabilities.

For each family distinguish:

- User's verb and object.
- Tool discovery vs executing an operation on supplied data.
- Optional target/context and whether it was explicitly supplied.
- Compatible existing actions.
- Confusable neighboring intents.
- Negative examples and unsupported requests.
- Read-only vs mutating effects.

Propose a compact synonym/alias format with ownership and versioning. Examples such as “pretty print” → JSON formatting or “checksum” → hashing are hypotheses until matched to real commands. Support abbreviations and common typo patterns without an uncontrolled synonym explosion.

Define ambiguity explicitly. “Compare” might mean files, folders, images, or text. A clear discovery result group or chooser is preferable to pretending one is certainly intended. “Delete duplicates” must not become an executable default merely because Duplicate Finder exists.

Specify empty input, very short input, token repetition, punctuation, case, Unicode, accents, locale assumptions, and malformed input. Do not silently expand to multilingual semantics unless the source/product scope supports it. An English-first design can document how additional locale catalogs would fit without implementing them.

## F. Deterministic ranking specification

### F1. Separate hard constraints from relevance

Eligibility, protected syntax, confirmation requirements, disabled plugins, unavailable targets, and unsupported capabilities are not ordinary scoring features. Apply the appropriate owner-defined rules before or alongside ranking so a high relevance score cannot bypass them.

Evaluate designs such as:

1. Existing routing/ranking unchanged plus curated aliases.
2. Deterministic weighted lexical/intent scoring with an explicit command catalog.
3. A compact token/character index or similarly justified local retrieval improvement.

Compare at most three meaningful prototype candidates. Do not add dense embedding dependencies merely to make the project look sophisticated. An offline precomputed semantic approach may be discussed as a rejected/deferred option if its deployment/maintenance cost outweighs benefit.

### F2. Specify the algorithm precisely

The final design must include:

- Candidate generation and eligibility ordering.
- Normalization rules and protected literal boundaries.
- Exact/prefix/token/alias/fuzzy match precedence.
- How verb/object coverage is combined and partial coverage penalized.
- How score scales differ across providers and how they are made comparable or kept separate.
- A bounded personalization contribution and cold-start behavior.
- Deterministic ties based on stable identity, not hash iteration or asynchronous arrival order.
- Duplicate-action/result collapsing that does not merge semantically distinct targets.
- Top-k selection and worst-case budget reasoning.
- No-result and low-confidence behavior; confidence labels must not masquerade as calibrated probabilities.

Prefer a lexicographic policy when it makes invariants clearer than a single unbounded weighted score. For example, study an ordered tuple of eligibility bucket, explicit-match class, semantic/lexical relevance, bounded usage adjustment, and stable ID. This is an illustration to evaluate, not a mandated final formula.

Where numeric weights are selected, publish their rationale and tuning set. Show sensitivity: can one frequently used action outrank an exact intended command? Can generic aliases overwhelm a precise object match? Can repeated tokens manipulate the score? Can a one-letter query trigger surprising recommendations?

### F3. Protect selection and interaction stability

Define what happens as results arrive asynchronously, user history updates, settings change, or the user moves selection. Do not reshuffle the selected result into a different action immediately before Enter. Specify selection by stable identity, invalidation when eligibility changes, and when new rankings may become visible.

Explain how the design interacts with current refresh tickets/generations, keyboard repeat, hotkey showing/hiding, Jump target registration, dashboard items, saved queries, and Universal Actions. Do not introduce a second execution authority to solve ranking.

### F4. Privacy-respecting personalization

Design an optional, bounded influence based on already available permitted metadata. Do not read the user's actual history/clipboard/settings secrets for this project. Prototype with synthetic counts and synthetic contexts.

Specify whether personalization is enabled by default, how to clear/disable it, migration from current formats, consistency when settings are saved, retention limits, and behavior when records are corrupted or missing. Do not add time-based inference unless the product benefit and new stored information are justified.

For any context-aware mode, define a clear user-controlled boundary. Do not poll active windows, clipboard, file contents, or application titles on every keystroke merely to improve relevance.

## G. Runtime and integration architecture

Produce a concrete proposed ownership diagram and interface sketches. Identify which current owner supplies metadata, which handles queries, which ranks, which owns cached state, and which still executes commands.

Specify candidate record and result record semantics, including IDs, owner identity, catalog revision, raw query/payload boundary, explanation tags, effect classification, and eligibility status. Proposed Rust type sketches are illustrative design, not compiled code or a demand to create those exact names.

Address:

- Static/curated catalog generation versus runtime registration.
- Cache lifetime, bounded size, invalidation, disabled plugin changes, and dynamic plugin unloading.
- Startup loading and no-work idle behavior.
- Query revision and asynchronous cancellation.
- Stale target rejection at execution.
- Versioning, schema migration, feature flag/rollback, and compatibility with existing plugin APIs.
- Preserving legacy exact-prefix and literal-input routes.
- Reusing Universal Action providers without executing them during discovery in an unsafe context.
- Whether a narrow pure core boundary is worthwhile, with a source-backed reason rather than automatic new crate creation.

State proposed engineering budgets with assumptions and how they will be measured later. Do not guarantee sub-millisecond Rust latency from a Python experiment. Distinguish catalog memory, transient ranking allocations, application-wide memory, and index build cost. No claim of reduced startup/idle cost without runtime measurements.

Use a staged migration proposal: static metadata and corpus first, side-effect-free ranking core second, opt-in UI integration next, then narrow compatibility/quality checks and controlled rollout. Adjust the stages if the observed code suggests a smaller safe change.

## H. Keyboard-first UX and safety

Describe real user interactions rather than only backend scoring. Include exact proposed behavior for:

- A known exact prefix.
- A descriptive query with one clear tool.
- An ambiguous query with multiple legitimate tools.
- An unavailable or disabled capability.
- An unsupported request with no safe match.
- A selected result opening a tool vs directly invoking an action.
- A result with multiple Universal Actions.
- Typing while results update.
- Jump Mode entering/cancelling/focusing without unexpected activation.
- Escape, Enter, Tab, arrow keys, and existing shortcuts where currently owned.

Use text wireframes or Mermaid state diagrams. Do not require screenshot generation, GUI launch, or redesign every dialog. Preserve one unified search experience; do not reintroduce the removed scope bar.

Treat discovery as recommendation, not authority. No fuzzy text should silently produce shell execution, deletion, replacement, unattended macro execution, or clipboard writes. Existing confirmations and previews remain enforced by current execution owners. For a potentially dangerous intent, prefer a safe tool-opening route or explicit additional selection and confirmation. Explain the policy precisely enough to test.

Threat-model malicious catalog/metadata strings, oversized aliases, unexpected Unicode, adversarial queries, stale action IDs, and action/provider disablement. No untrusted metadata may become instructions to the AI during research or executable strings in the prototype.

## I. External comparison and original engineering judgment

Use official documentation for Raycast, Alfred, Flow Launcher, and PowerToys Command Palette only where it answers a real question about discovery, actions, keywords, ranking customization, or extensibility. Limit the comparison to relevant interaction and architectural tradeoffs.

Record publication/access date, exact supported observation, applicability to a Windows Rust/egui launcher, and a reason not to copy an incompatible pattern. Distinguish documented behavior from inference. Do not rely on promotional claims as performance evidence or copy third-party code/assets into the project.

If browsing is unavailable, finish a source-grounded design and label competitor comparison unverified. External comparison is support for decisions, not a gate blocking the goal.

## J. Offline prototype and evaluation

### J1. Prototype boundary

Build a small, readable standard-library Python prototype under `prototype/` that accepts only the synthetic/catalog data, returns ranked candidate IDs/explanations, and does not execute actions. No service, UI framework, database, package installation, model download, network, user-history loading, or subprocess call to Cargo.

Suggested components, to simplify or combine when appropriate:

```text
prototype/catalog.py       # validate/load the explicit synthetic catalog
prototype/ranker.py        # baseline and selected candidate policies
prototype/evaluate.py      # fixed splits, metrics, error examples, result output
prototype/smoke_checks.py  # small invariant checks only
```

Do not create a general experimentation platform. Reproducible command-line entry points and stable seeds are enough. Support Windows-friendly paths and avoid dependencies unavailable in the current environment. Keep scripts within docs and write generated results there.

### J2. Dataset and labels

Target roughly 300 genuinely distinct, source-grounded synthetic query cases, with at least 250 when practical. Do not meet a numeric target by mechanically changing punctuation or duplicating templates. If source scope justifies fewer cases, explain coverage rather than padding.

A reasonable starting distribution to refine before generation is:

| Family | Initial target |
|---|---:|
| Exact prefixes, literal payloads, and existing syntax | 70 |
| Natural descriptions and synonyms for real tools | 80 |
| Abbreviations and realistic typos | 40 |
| Ambiguous requests with several acceptable outcomes | 35 |
| Target/context availability and disabled capabilities | 30 |
| Unsafe, unsupported, or sensitive-data-like input | 25 |
| Empty/short/Unicode and state-transition edge cases | 20 |
| **Total** | **300** |

These are coverage targets, not a fixed gate or an incentive to invent unsupported commands.

Each case records: ID, query, family/intent cluster, source references, synthetic context/enablement, expected route, acceptable top results with graded relevance when useful, forbidden outcomes, expected abstention/chooser behavior, and labeling provenance/confidence. Multi-answer cases must not falsely label one arbitrary result as the only correct answer.

Use synthetic paths, names, query payloads, and usage counts. Clearly label the dataset model-authored, not human ground truth. Do not claim it represents real user distribution or observed behavior.

### J3. Prevent evaluation leakage

Split by semantic intent/template family rather than randomly scattering near-duplicate paraphrases. A proposed 60/20/20 development/validation/held-out split is acceptable if the small dataset supports it; record the actual counts and seed.

Freeze labeled cases and split before final tuning. Prefer reviewer-authored held-out examples not supplied to the ranker-authoring role during tuning. If the same parent had visibility into all labels, say so; do not call that an independent blind test. Reserve some unsupported and ambiguous families for the final evaluation.

Do not keep retuning on the held-out results. After one final evaluation, a proposed next improvement goes into follow-on work or a clearly labeled exploratory result, not a quietly reused “test” score.

### J4. Baselines and metrics

First document what the *current* application actually does. If recreating it faithfully without Rust execution is infeasible, name the Python baseline `documented-policy surrogate` rather than “current launcher performance.” An exact/prefix-only simple baseline is useful but must not be misrepresented as the full existing ranker.

Evaluate only what the prototype implements. Relevant measures include:

- Exact/prefix/literal routing preservation against explicit fixtures.
- Top-1 and top-k acceptable-action accuracy for answerable discovery queries.
- Mean reciprocal rank and/or nDCG when relevance labels support them.
- Ambiguity handling and abstention behavior.
- Forbidden/unsupported/disabled action suggestion rate.
- Stability of ties and rankings under repeated identical input.
- Bounded influence of synthetic usage counts.
- Per-family errors, not just one aggregate score.
- Optional wall-clock and operation counts on synthetic input, labeled as Python-only observations with environment/context.

Publish denominators, exclusions, ties, unavailable results, and whether data is generated or observed. A score is not meaningful without its task and label definition. No fabricated significance claims from one small synthetic run.

### J5. Lightweight execution limits

Follow the orchestrator's caps: no more than three predeclared ranking candidates, small smoke/invariant checks, one initial evaluation per candidate and at most one evidence-backed corrective reevaluation, 120 seconds per evaluation and 10 minutes aggregate prototype execution. Do not brute-force a large hyperparameter grid.

If a computation is too slow, reduce nonessential synthetic scale and preserve comparable settings. If execution cannot run, provide finished code/data/protocol and `NOT_RUN` results with reproduction commands; continue all analysis and follow-on authoring. Do not guess output numbers or block the entire research goal.

### J6. Required result interpretation

Explain what improved, did not improve, or was inconclusive, and why. Include representative good and bad queries, score explanations, failure clusters, privacy/safety implications, and sensitivity to catalog assumptions. Never hide a no-gain result by replacing the baseline after evaluation.

Distinguish four levels: design plausible, prototype correct on its invariants, relevance supported on synthetic cases, and production Rust/native behavior still unverified. Only the levels actually established may be claimed.

## K. Follow-on work library

For each recommended change, generate complete canonical `GOAL.md` and `KICKSTART.md` using the program templates. Source paths must be revalidated at authoring time and future packages must instruct revalidation again before implementation against then-current master.

Potential package boundaries to evaluate, not automatically manufacture:

- A single-source command/discovery metadata catalog preserving current plugin interfaces.
- A deterministic pure ranking core plus exact-route compatibility contract.
- Query/result UI integration with stable selection and asynchronous invalidation.
- Bounded opt-in personalization and persistence migration.
- Contextual discovery using existing Universal Actions without a second dispatcher.
- A quality corpus/evaluation bridge into future focused Rust tests.
- A narrow research spike for an unresolved semantic or latency tradeoff.

Each package must identify current owners and exact entry points, proposed records/APIs, data/control flow, algorithm, defaults, migration and rollback, unsafe/non-goal cases, proposed performance budgets, dependency graph, small commit milestones, targeted future tests, and real completion criteria. Do not demand a full native campaign for a pure metadata-only change, but do not substitute unit proof for a genuinely changed focus/input/output contract.

Every accepted recommendation has a pair. Role-authored proposals live under their own role folders and are consolidated into one canonical plan per actual problem. Links back to relevant audit packages must be explicit. A speculative idea becomes a research-spike goal with a decision output, not a falsely implementation-ready feature.

For each recommendation, select models by difficulty: Astra for ambiguous architecture/safety/review; Sol for bounded, clear implementation where appropriate. Explain the choice and retain a supported fallback. Model selection does not make a package authorized to execute.

## L. Checkpoint plan

### M0-A — New branch and audit-informed baseline

Create the new Goal 02 branch from Goal 01's recorded closing commit. Read its handoff, establish output paths and role provenance, record source SHA and carried constraints. Commit the goal baseline before substantial writing.

**Commit:** `docs(research): [G02/M0-A] establish search research baseline`.

### M1-A — Existing search and real capability catalog

Trace actual routing, candidate generation, ranking, refresh, usage and execution. Produce `CURRENT_SEARCH_ARCHITECTURE.md` and source-backed catalog, with usable vs deferred capabilities and protected syntax.

**Done:** no assumed missing ranker, invented command, or unsupported tool advertised as usable; actual query behavior is mapped.

**Commit:** `docs(search): [G02/M1-A] map search owners and capability catalog`.

### M1-B — Intent, ranking, and runtime design

Produce taxonomy, precise ranking alternatives, score/tie/eligibility rules, proposed runtime ownership, caches and budgets. Write adversarial examples and design decision records. Trace the compatibility contract into actual owners.

**Done:** an implementer can explain candidate generation, filtering, selection stability and execution boundaries without rediscovering the design.

**Commit:** `docs(search): [G02/M1-B] specify deterministic discovery architecture`.

### M1-C — UX, safety, and evaluation protocol

Produce interaction specifications, text wireframes, privacy and failure policy, relevant external comparisons, synthetic labels and split protocol. Freeze catalog/corpus and predeclare comparison candidates.

**Done:** exact/literal routes and destructive intents have explicit behavior; dataset claims and labels are honest; evaluation is not self-congratulatory examples only.

**Commit:** `docs(search): [G02/M1-C] define keyboard UX and relevance evaluation`.

### M2-A — Small prototype and bounded experiments

Implement the standalone prototype, a small invariant check, and predeclared evaluations when tooling permits. Record commands, environment, data/config hashes or stable identities, real results/errors and limits. Keep the allowed execution budget.

**Done:** runnable code/data and true execution disposition exist; NOT_RUN is explicit when applicable; no Rust/native/production action executed.

**Commit:** `feat(research): [G02/M2-A] add isolated search relevance prototype`.

### M2-B — Interpret evidence and choose the design

Analyze per-family results, failed cases, stability, sensitivity and product tradeoffs. Choose a candidate or an evidence-backed no-change/spike outcome. Finalize `EVALUATED_EXAMPLES.md`, decisions, recommendations and implementation sequencing.

**Done:** improvement claims match actual evidence; uncertainty and counterexamples remain visible; speculative runtime budgets remain targets.

**Commit:** `docs(search): [G02/M2-B] evaluate tradeoffs and select bounded search design`.

### M3-A — Full follow-on packages and cross-goal reconciliation

Create all role and canonical Goal/Kickstart pairs. Reconcile audit overlaps; produce exact dependencies and testing scopes. Fill per-goal and program follow-on indexes with proposed-not-executed status.

**Done:** every actionable recommendation is implementer-ready or a concrete bounded research spike; no orphan finding, contradictory shared-owner plan, or generic TODO remains.

**Commit:** `docs(search): [G02/M3-A] package implementation goals and audit dependencies`.

### M3-B — Critical review and final program handoff

Reviewer challenges source claims, algorithm failure cases, benchmark labeling/leakage, privacy/safety, and package quality. Apply one substantive correction pass; preserve unresolved choices honestly. Complete program summary, priority roadmap, PR draft, branch/push record and next instructions.

**Done:** all core artifacts navigable; no unauthorized application changes; both research goals truthfully classified; no automatic future implementation or master merge.

**Commit:** `docs(research): [G02/M3-B] close reviewed research program and handoff`.

## M. Acceptance and explicit non-acceptance claims

Accept the research when current-system facts, actionable design, alternatives, catalog and examples, evaluation assets/results disposition, future work packages and review/limits are complete. A candidate failing to improve the synthetic benchmark does not fail the research if the findings are useful and the recommendation changes accordingly.

Do not claim production speedups, human-validated semantic understanding, complete Windows behavior, all plugins covered at runtime, or readiness to merge a search implementation. This goal produces the evidence and plan for later implementation, not that implementation.

Nonblocking limitations follow the orchestrator policy. Continue unrelated work when an agent, browser, optional measurement or external source is unavailable. Preserve a precise resume point for genuine session or safety stops.

## N. Execution progress — mutable

**State:** RESEARCH_COMPLETE_WITH_LIMITATIONS — all research, full packages, separate review, one correction pass and scoped final checks complete.
**Program ID / GOAL_ROOT:** multi-launcher-research-20261028-r01 / docs/research/multi-launcher-research-20261028-r01/02-search-discovery
**Source SHA / branch / worktree:** 59156d96adcc73939c7171487a0cb582e3286587 / codex/research-search-discovery-20261028-r01 / G:/Repos/rust/Multi_Launcher
**Audit handoff commit:** 3b79d73f12a791b8b077ed0bf7d6631bd885b68c; Goal01 closing documentation, normally pushed.
**Active checkpoint / role / actual model:** M3-B parent closeout; all children complete. Separate reviewer requested/configured Astra max; serving backend/effort unverified.
**Last known commit / push:** 3a4924e20a95de38af3ba60aca6b86ade1e1c0f6; M3-A accepted and normally pushed.
**Next action:** publish closing checkpoint and report exact SHA/push; then await future package selection. Exactly two proposed search packages; no automatic future execution, merge or evaluation cycle.

[Authoritative program status](../../research/multi-launcher-research-20261028-r01/PROGRAM_STATUS.md)

| Checkpoint | State | Actual artifacts/evidence | Known commit/push | Next action or limit |
|---|---|---|---|---|
| M0-A | COMPLETE | [Baseline/index](../../research/multi-launcher-research-20261028-r01/02-search-discovery/README.md), clean exact-base branch, parent provenance | 9ef37074 / pushed | Bounded planning |
| M0-B | COMPLETE | [Planner handoff and complete raw SEARCH-F001 pair](../../research/multi-launcher-research-20261028-r01/02-search-discovery/roles/planner/ANALYSIS.md) | a6a16395 / pushed | Proposed foundation only; no implementation |
| M1-A | COMPLETE | [Architecture](../../research/multi-launcher-research-20261028-r01/02-search-discovery/CURRENT_SEARCH_ARCHITECTURE.md), [catalog](../../research/multi-launcher-research-20261028-r01/02-search-discovery/COMMAND_AND_CAPABILITY_CATALOG.md), JSON/role analysis; static checks complete | 80b2050a / pushed | 15 research descriptors, 11 initial / 4 research-only; no evaluator |
| M1-B | COMPLETE | [Grammar](../../research/multi-launcher-research-20261028-r01/02-search-discovery/INTENT_TAXONOMY_AND_GRAMMAR.md), [ranking](../../research/multi-launcher-research-20261028-r01/02-search-discovery/RANKING_SPECIFICATION.md), [runtime](../../research/multi-launcher-research-20261028-r01/02-search-discovery/RUNTIME_AND_INTEGRATION_DESIGN.md), three policy definitions/decisions; static checks pass | cc3da667 / pushed | UI completeness/reachability gap explicit; no ranker execution |
| M1-C | COMPLETE | [UX](../../research/multi-launcher-research-20261028-r01/02-search-discovery/KEYBOARD_AND_RESULTS_UX.md), privacy/official comparisons, 300-case corpus/splits and [protocol](../../research/multi-launcher-research-20261028-r01/02-search-discovery/evaluation/EVALUATION_PROTOCOL.md); 17 hashes/static checks passed | ea453edf / pushed | 173/61/66 author-visible split; v1 freeze immutable |
| M2-A | COMPLETE | [Execution/receipts](../../research/multi-launcher-research-20261028-r01/02-search-discovery/evaluation/M2A_EXECUTION.md), pure stdlib prototype, all three policies/both scopes; one Q122 label-only overlay/measurement correction | 67df35cf / pushed | Guarded 2.8697286s/600; all 1,800 predictions unchanged in correction; M2-B interpretation follows |
| M2-B | COMPLETE | [Results](../../research/multi-launcher-research-20261028-r01/02-search-discovery/evaluation/RESULTS.md), [examples](../../research/multi-launcher-research-20261028-r01/02-search-discovery/EVALUATED_EXAMPLES.md), SD12/SD13; saved metrics/decompositions and all top1 gain/loss IDs | a1bd7875 / pushed | Conditional P2; no production change; F002 decision before conditional F001 |
| M3-A | COMPLETE | [Full canonical index](../../research/multi-launcher-research-20261028-r01/02-search-discovery/FOLLOW_ON_INDEX.md), complete raw/canonical F002 and amended canonical F001 pairs, module/bound/dependency amendment, recommendations/cross-goal/review handoff/provenance | 3a4924e2 / pushed | Exactly two proposed search packages; separate review active; no future execution |
| M3-B | COMPLETE | Separate review of 3a4924e2, one F002 scope clarification, full handoff and passing affected checks | Closing commit containing this progress; exact SHA/push in final handoff | Research complete with limitations; future packages not authorized |

### Material events and decisions

- 2026-10-28: Goal01 closed after separate review and parent correction. Fresh Goal02 branch begins at exact closing SHA; production remains pinned MASTER_START. Read corrected search handoff and carried constraints. No application/prototype execution at baseline.

- 2026-10-28: Planner contribution and full raw SEARCH-F001 pair persisted under roles/planner. Metadata-only foundation proposed; ranking/UI remain design targets. Parent official comparison evidence and initial cross-program catalog saved. No application/prototype execution.

- 2026-10-28: M1-A mapped current routing/filter/fallback/scoring/usage/pending/activation and automatic consumers, preserving corrected g/capacity/post-filter semantics. Static catalog contains 15 real fixed opener Actions across 11 owners; 11 initial descriptors retain raw SEARCH-F001 scope, four research-only Diff/File Search openers require separate later metrics/scope decisions. Five unsupported meanings are non-executable. No providers/user stores/evaluator/prototype invoked, no new actionable author recommendation, no Goal01 or production edits. Static checks/limits are recorded in author ANALYSIS; parent publication is next.

- 2026-10-28: M1-B defines hard admission/finite English grammar, exact P0/P1/P2 integer policies/hand calculations, zero personalization, immutable selection/revalidation, synchronous bounds and owned opt-in settings. Parent survey identifies 81 builtin registrations/45 manager-global; 43 global contracts remain unclassified beyond Web/Calculator. Unknown includes disabled owners and blocks discovery; default-like reachability remains an explicit integration acceptance/spike gap. No new accepted production package, evaluator/prototype or raw planner/Goal01/source changes.

- 2026-10-28: M1-C authors complete UX/privacy/compatibility and four official-page comparisons; 300 meaningful synthetic cases with grouped 173/61/66 split, author-visible labels, identical subset denominators and portable UTF-8/LF freeze rules. Pure hypothetical Complete contexts do not resolve 43 global grammar/live reachability gaps. Numerical candidates remain unchanged, no ranker/evaluator code or execution and no accepted new production package. Parent publication must precede M2-A.

- 2026-10-28: M2-A executes one 26-assertion smoke and one P0/P1/P2 initial invocation within bounds, preserving all 17 frozen v1 inputs/code/results. Single Q122 reserved-data-head label error receives a versioned overlay: 119 positives/164 gated, same 300 cases/splits and both scopes; sole correction is measurement-only with zero ranking calls and 1,800 predictions/payloads unchanged. Guarded total 2.8697286s/600. All policies/scopes have 300/300 amended synthetic gate agreement, zero forbidden/ineligible output, retained relevance/chooser failures. Original raw source bytes recoverable alongside canonical hashes. No native/production/provider action, extra cycle or final candidate recommendation; parent publishes before M2-B.

- 2026-10-28: M2-B reads saved outcomes without prototype execution or frozen edits. Applies predeclared floors: P2 conditional reference in both scopes, holdout 6/18 versus P1 5/18/P0 1/18, sole additional holdout hit Q026; weak chooser 7/30 full and 3/30 initial, Q132 full top1 regression and semantic misses remain. Current production recommendation is no search/ranker/UI change. Proposed SEARCH-F002 bounded admission/useful-consumer/compatibility decision proof precedes conditional eleven-descriptor F001; full pairs/canonical owner/bound amendment await M3-A, no ranker/UI package quota or automatic execution. Synthetic Complete does not resolve 43 global grammar/live/native gaps.

- 2026-10-28: M3-A completes full raw author F002 decision-proof and exactly two canonical search pairs, preserving full raw planner F001. Both canonical F001 documents require explicitly accepted concrete useful consumer/compatibility version/F002 closing commit plus separate authorization; favorable source-only classification is insufficient. Single src/search_discovery and narrower field/text shipping bounds are reconciled by a separate amendment without frozen-input edits. F002 is bounded docs/source-contract work, no-change allowed, optional ≤128-state model/≤30s per invocation/≤60s aggregate/one initial plus one localized correction (maximum two). Index/recommendations/cross-goal/review handoff and promotion records complete; no ranker/UI package quota, source/prototype/native execution, Goal01 edit or future goal run. Separate review follows parent publication.

- 2026-10-28: M3-B complete. Exact reviewer provenance/report persisted; six metric rows/1,800 unchanged predictions and key source boundaries corroborated without rerunning prototype. One canonical F002 scope clarification applied; raw/frozen/promotion evidence preserved. Parent affected checks passed (19 Markdown/242 links/18 pinned URLs/two JSON/both companions), with only research/progress paths changed. Both goals close RESEARCH_COMPLETE_WITH_LIMITATIONS; four canonical packages proposed, none executed. Closing subject recorded in PROGRAM_STATUS; final SHA/push reported after publication.
