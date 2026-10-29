# Goal SEARCH-F002 — Decide a useful discovery admission and consumer contract

## 0. Identity, provenance and readiness

- **Package/recommendation:** SEARCH-F002; source decision SD13, Goal02 Intelligent Search and Action Discovery.
- **Canonical directory:** `docs/research/multi-launcher-research-20261028-r01/02-search-discovery/follow-on/SEARCH-F002-discovery-admission-consumer-proof/`.
- **Raw author directory:** `roles/implementer/follow-on/SEARCH-F002-discovery-admission-consumer-proof/` under Goal02; this is that full raw proposal, preserved after authorship.
- **Type/state/readiness:** bounded research/contract decision spike; `PROPOSED_NOT_AUTHORIZED`; `NEEDS_RESEARCH_SPIKE`. The spike is sufficiently scoped for execution review when separately selected; no production design is declared accepted.
- **Priority/value:** first search follow-on, before conditional SEARCH-F001. Resolve whether safe useful admission and a concrete consumer justify maintaining discovery infrastructure, or explicitly choose no-change. User-efficiency benefit is hypothetical; the reference only suggests potential utility.
- **Severity/confidence/complexity/risk:** no reproduced production defect. High confidence in the documented registration/coverage gap and existing owners; uncertain usable/default-like reachability. Moderate bounded reasoning complexity, low application regression risk because production is read-only, meaningful review burden for completeness and authority claims.
- **Historical source:** `59156d96adcc73939c7171487a0cb582e3286587`; research M2-B `a1bd78758f61f3fc9dd5a96a57b1d5280d1b3a2b`. Both are provenance, not a required future checkout.
- **Author/contributors:** `/root/audit_author`, 2026-10-28, accepted worker request `gpt-6.1-sol`/`xhigh`; serving identity/effort unverified. Uses planner ownership map, parent's 81-owner declaration survey and closed audit corrections. Research independent review is pending; future spike acceptance does not exist.
- **Source records:** [SD12/SD13](../../../../DECISION_RECORDS.md), [results](../../../../evaluation/RESULTS.md), [current architecture](../../../../CURRENT_SEARCH_ARCHITECTURE.md), [routing survey](../../../../roles/orchestrator/ROUTING_METADATA_SURVEY.md), [historical runtime design](../../../../RUNTIME_AND_INTEGRATION_DESIGN.md), [canonical amendment](../../../../CANONICAL_DESIGN_AMENDMENTS.md).
- **Companion:** [KICKSTART.md](KICKSTART.md). [Conditional SEARCH-F001](../../../../follow-on/SEARCH-F001-built-in-discovery-metadata/GOAL.md) is a downstream proposal, never an execution step of this spike. [Closed audit handoff](../../../../../01-static-audit/SEARCH_HANDOFF.md) supplies constraints; AUD-F001/AUD-F002 remain independent.

## 1. Objective and observable outcome

Produce one source-grounded **ADMISSION_DECISION.md** and concrete **CONSUMER_COMPATIBILITY_CONTRACT.md**, or an equally explicit **NO_CHANGE** decision explaining why no useful bounded consumer is justified. This is a speculative-opportunity decision, not a production fix. All launcher behavior remains unchanged. A synthetic query such as `format xml` can score well only after hypothetical Complete coverage and canonical-empty/current state; the current evidence does not show ordinary users actually reach that state safely or usefully.

The positive outcome must name a real source-plausible registered configuration and a concrete fresh-user main list/grid consumer with preserved canonical/automatic behavior. It must distinguish source-admissible conditions from live/provider/native behavior still requiring later focused proof. A favorable prefix/body classification alone cannot count as consumer acceptance, prove native behavior or authorize metadata/ranker/UI work.

The negative outcome is a successful research result: keep current production search, retain useful documentation, recommend no infrastructure execution and identify the concrete premise that failed. Do not force complete grammar infrastructure or another recursive implementation queue to avoid that result.

## 2. Current architecture and source evidence

All URLs below pin the historical evidence source; revalidate current owners before analysis. Existing mechanisms are real and must be reused in any prospective contract.

| Owner / pinned source | Existing responsibility | Spike question / output |
|---|---|---|
| [Plugin defaults/contract, src/plugin.rs:112–151](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L112-L151); [manager admission1411–1437](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1411-L1437) | Empty prefixes are manager-global; always_search also bypasses a simple head gate. Declared capability and configured policy differ. | Classify actual guarded-head/global-content/global-grammar/Unknown boundaries; never infer arbitrary grammar from global routing alone. |
| [reload_from_dirs876–1008](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L876-L1008); [snapshot1108–1159](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1108-L1159) | Registration/origin/epoch and immutable revisioned metadata already exist. Baseline81 registrations:36 prefix-gated,45 global,43 global bodies unclassified beyond Web/Calculator. | Reconcile current roster and bind each reviewed contract to exact origin/name/source revision. Dynamic/registered/duplicate/stale identity stays Unknown/ineligible as applicable. |
| [Web/Calculator, src/plugins_builtin.rs:10–110](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins_builtin.rs#L10-L110); [Snippets880–890](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/snippets.rs#L880-L890) | Restricted global grammars already have load-bearing source evidence; Snippets reserves adjacent cs syntax via always_search. | Preserve those distinctions; an allowlist/head helper cannot erase disabled or adjacent protected syntax. |
| [src/settings/model.rs:1136–1150](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings/model.rs#L1136-L1150) | Default enabled-provider/capability policies are None/None; actual user's state was not observed. | Decide source-plausible default-like and deliberately restricted configurations separately. Unknown includes disabled owners under the proposal. |
| [src/gui/search.rs:707–767](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L707-L767); [1245–1300](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1245-L1300); [1125–1667](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1125-L1667) | Query/filter/Literal/fallback, canonical result/pending/catalog state and Macro query-resolution/broker compatibility have actual owners. | Trace only the necessary consumer boundary; keep self.results, command cache, completion, Macro instantaneous zero/one/many, queryexec first row and programmatic routing unchanged. Capacity deferral is not confirmed no-match. |
| [src/gui/actions.rs:69–187](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/actions.rs#L69-L187); [Action fields12–19](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/actions/mod.rs#L12-L19) | Current ActivationSource, ActionSurface and invocation context carry distinct authority. Action/args are existing payload identity. | A proposed query-origin type is new policy, not an existing enum. Specify exact immutable binding/revalidation/explicit selection without claiming it is implemented. |
| [Settings save intent, src/settings_editor/save.rs:12–66](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings_editor/save.rs#L12-L66); [src/settings_save.rs:830–1007](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings_save.rs#L830-L1007) | Draft→owned SaveIntent→acknowledged policy application already exists. | Any prospective false-default opt-in must reuse it, with invalidation/rollback; no new persistence path in this spike. |

The source survey is declarations only, not a complete route manifest or body audit. Source-backed global content matching can be reviewed as such; global/default dispatch alone proves neither safety nor danger. Current table core, typed commands, Settings catalog, input authority and native owners are not missing abstractions. P2's holdout gain is one case, chooser 7/30 full and 3/30 initial; no infrastructure mandate follows.

## 3. Scope, non-goals and dependencies

**Writes:** only `docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/` for the execution Goal/Kickstart, source manifest, decision/consumer contract, optional proof source/receipts, role/progress/review/stall files. **Reads:** named current production owners, the 81-row historical inventory and directly required provider bodies/helpers for their grammar, current tests as source, historical research/audit evidence. Production/config/manifests/test sources remain read-only.

No production Rust, Cargo/rustc/Nextest/Clippy, native/app test/benchmark, provider/parser execution, live plugin construction, user settings/history/clipboard/files, network/subprocess/dependency installation, metadata/ranker implementation, source modification or new plugin ABI/dispatcher. Do not rerun Goal02's exhausted relevance prototype or reuse its holdout for tuning. The optional proof below is a separate future synthetic admission-state model only, not a fourth ranking policy or current research execution permission.

Independent future branch from **then-current verified master**, or explicitly approved integrated base. Neither audit follow-on nor F001/source changes are prerequisites. Revalidate current roster and exact source assumptions, do not require historical branch ancestry. If equivalent accepted contract already exists, verify its evidence/version and record a no-op/narrow remaining decision; do not create a second manifest owner.

## 4. Hypotheses, bounded evidence and decision design

### Competing outcomes and deliverable schema

- **H0 / NO_CHANGE:** useful safe admission cannot be established within the bounded source contract, the only proposed consumer is not worth maintenance/ambiguity cost, or broad infrastructure/runtime exploration would be required. Record the exact blocker/counterexample and preserve current behavior. Unknown is not relabeled Complete.
- **H1 / CANDIDATE_CONTRACT_REQUIRES_ACCEPTANCE:** current source supports a concrete bounded consumer/configuration, reviewed coverage and compatibility plan with explicit remaining live/native limits. This recommends a reviewable contract, not infrastructure execution.
- **H2 / ALREADY_SATISFIED:** equivalent accepted capability/contract already exists on current master. Cite exact owners/accepted decision and narrow any real delta; do not duplicate it. Treat a merely prospective historical design as unsatisfied.

`SOURCE_ADMISSION_MANIFEST.md` records every current registration or explicit delta from the historical 81 rows: name/type/origin; prefix/always-search declarations; owner/source hash and locators; source-reviewed category; raw protected forms including adjacent/special syntax; route-affecting policy; dynamic/duplicate/stale behavior; evidence depth; unresolved condition and conservative disposition. Categories are `HEAD_GATED`, `REVIEWED_GLOBAL_CONTENT`, `REVIEWED_GLOBAL_GRAMMAR`, `UNKNOWN`. These are proposed document categories, not a Plugin trait extension.

`CONSUMER_COMPATIBILITY_CONTRACT.md` must name an exact proposed consumer, source insertion/read boundaries, useful configuration and query sequence, canonical empty/current/pending/deferral conditions, declared coverage, query-origin transition, selection/binding/eligibility invalidation, exact opener Action/args restrictions, Settings/save ownership, no logging/payload inference, rollback and focused future acceptance. It must state what is source-established, merely modeled, not run, or requires future live/native proof. Even a NO_CHANGE result retains the rejected candidate and exact failed condition rather than a generic TODO.

`ADMISSION_DECISION.md` records H0/H1/H2, evidence links/versions, allowed/rejected alternatives, counterguards, unknown cost, whether the intended source-plausible configuration is reachable under the contract, and a separate acceptance block. **Contract acceptance requires an explicit user or already-authorized product decision recorded against the exact contract version and F002 closing documentation commit.** Reviewer/source classification agreement alone is not that decision. No acceptance is assumed from elapsed time. If not accepted, mark `CONTRACT_NOT_ACCEPTED`, do not recommend F001 execution, and close the spike truthfully with its evidence. This does not prevent completing documentation or publishing the result.

### Bounded source pass

1. Revalidate the current manager defaults/registration/snapshot/admission and named GUI/capability/save owners once. Reconcile baseline 81 with the actual roster, up to the historical design's 256-owner review ceiling. A larger roster is an explicit limit/no-change outcome, not authorization to scale the audit.
2. Revalidate the 36 historical head-gated declarations and the manager-level head boundary; preserve those heads even if owners are disabled. Revalidate two already-reviewed global owners. Review the 43 formerly unresolved global entry bodies and only their directly grammar-defining helpers as needed. **One pass, at most 45 global owner entry points and one direct helper layer per owner.** A changed/new/opaque owner, deeper unresolved helper/config grammar or unbounded dependency remains UNKNOWN. Positive completeness may require all current owners; no requirement forces exhaustive recursive reading to obtain it.
3. Distinguish ordinary global content matching from syntax/operations. Record current-source negative/no-match paths relevant to a candidate query and a safe synthetic data-empty condition without invoking providers. If canonical-empty is assumed rather than source-established, say so and do not call it live reachability. Classifying declarations alone is insufficient.
4. Check builtin identity/revision, disabled reserved syntax, adjacent Snippets, explicit app/g routes, Literal/filter/payload precedence, path/URL/regex/calculator/shell/quoted shapes and stale/unloaded/dynamic states at the source owners already mapped. No hash-only allowlist may evade disabled Unknown. Any completeness claim must cover all owners required by that configuration, or be rejected.
5. Develop at most **two candidate consumer/configuration contracts**: ordinary/default-like and one explicitly restricted useful alternative. Both must use the same safety/invalidation rules and preserve actual automatic consumers; no synthetic empty/Complete dictionary can serve as proof of live coverage. If both fail, H0 is valid. Do not broaden to a new metadata platform or additional surfaces to manufacture utility.

The source pass is complete when every required row is classified or explicitly UNKNOWN with the resulting admission cost, and one of H0/H1/H2 is justified. It need not turn all UNKNOWN entries into implemented grammar. If bodies exceed this bound, retain no-change/inconclusive source premise with a concrete reason; do not renew the pass automatically.

### Proposed data flow, state and adverse sequences

For reasoning only: existing manager facts + reviewed versioned builtin contracts → immutable coverage/eligibility snapshot → proposed fresh-user main list/grid admission after canonical current/empty/not-pending/not-deferred → separate discovery presentation → explicit immutable selected Action binding → current identity/policy/surface revalidation before existing activation. `src/search_discovery` is the single prospective metadata/domain owner; manager owns registration facts, GUI owns origin/presentation/binding, existing activation/commands own effects. No such source code is written here.

Compare raw reserved syntax before normalization. A complete route contract is separate from whether a descriptor is eligible. Disable/change capability after selection invalidates the row; same-name replacement/new epoch never rebinds it. A provider pending or Capacity-deferred result rejects discovery rather than supplying false empty evidence. A programmatic query cannot become FreshUserEdit merely by rendering. Dynamic/registered/duplicate/unloaded/stale coverage must retain Unknown/tombstone cost as explicitly chosen; if that cost makes the consumer useless, reject the design rather than claim completeness. Existing worker retirement differs from result rejection and is not “solved” by a snapshot model.

No user schema/persistence, UI event, focus/modal transition, callback, thread, cancellation, shutdown or resource lease changes occur. The contract names future ownership/acceptance, including real ActivationSource versus ActionSurface and proposed origin, without inventing a new dispatcher or moving domain logic into rendering. Opening an existing tool can retain state; no blank/no-work opener guarantee is made.

### Optional isolated proof and resource bounds

If a concrete ambiguity in the proposed admission-state contract benefits from executable checking, author a standard-library-only `proof/admission_model.py` inside the execution directory. Pure in-memory synthetic facts only: known/Unknown coverage, eligibility/origin/revision, canonical/pending/deferral state and immutable selected binding. **At most128 explicit states; exactly one initial invocation, plus at most one localized correction invocation only for a concrete model/transcription defect; maximum two invocations total. Each≤30 seconds, aggregate≤60 seconds.** Record commands, input/code canonical hashes, Python/sys.platform, elapsed time, assertion failures, correction reason and original output. Use `python -X utf8 -B`; no network/subprocess/providers/dependencies/native or Rust fallback. No scoring/ranking/held-out relevance cases, grid search or alternative framework.

If Python is absent, the initial attempt fails/times out, the bound is reached, or no concrete modeling need exists, retain `NOT_RUN`/`FAILED`/`TIME_LIMIT`/`NOT_NEEDED` with exact limits and continue the source/contract decision. One localized repair is permitted only within the two-run/60-second aggregate cap; no repair after exhausted time and no repeated tool-availability probes. A failed model does not establish a production defect. A passing model proves declared finite transitions only, not runtime coverage. Required acceptance is a truthful decision/contract with limits, not a passing optional script.

Migration is documentation-only: add current evidence and explicit supersession references without overwriting historical raw/frozen reports. If no-change is selected, no metadata, settings or consumer migration follows. Rollback is removal/supersession of task-owned docs/optional proof only, preserving research evidence and unrelated changes.

## 5. Alternatives, costs and rejection conditions

| Option | Cost / value / disposition |
|---|---|
| Keep source/docs and no production change | Valid baseline and H0. Prefer when safe useful admission or accepted consumer cannot be established. No unused infrastructure. |
| Existing enabled-prefix helper or F001 metadata alone | Insufficient: global grammars, disabled reserved syntax, dynamic origin/revision and GUI origin are distinct. Reject false completeness. |
| Narrow reviewed manifest + existing owners | Candidate contract only; maintenance cost is reviewed source-specific coverage/invalidation. Accept for further product review only if a concrete useful configuration/consumer and compatibility boundary exist. |
| Change canonical search/cardinality or enable discovery for automatic callers | Reject: current Macro zero/one/many/queryexec and explicit grammar are required compatibility. |
| General grammar/plugin ABI platform, extra dispatcher/cache/workers | Reject without a separately authorized need; not necessary to finish this decision. |
| Ship P2/metadata first, investigate admission later | Reject sequencing; conditional synthetic gains and one-case holdout sensitivity do not justify unused infrastructure. |

Reject H1 if any required owner remains opaque under the chosen configuration, disabled syntax is omitted, supposed empty canonical results actually mean pending/deferral, origin/binding authority is inferred, no useful source-plausible route exists, or compatibility requires broad source redesign. A source-plausible contract may remain H1 with unrun live/native evidence explicitly bounded; it still cannot authorize implementation without explicit acceptance and separate package selection.

## 6. Invariants and acceptance

1. Production source/config/data and all historical research inputs/code/results remain unchanged. No Cargo/native/provider/user-data action occurs.
2. Current owner/registration denominator and every evidence classification are auditable; UNKNOWN is not silently promoted to Complete, including disabled owners.
3. At most45 global entry points/one helper layer each and at most two consumer/configuration alternatives are investigated; expansion or unbounded recursion is not a hidden requirement. Changed/new/opaque owners retain documented Unknown.
4. A positive candidate identifies a source-plausible useful configuration and exact consumer/compatibility contract; it separates source inference, hypothetical state, optional proof and future runtime/native evidence. H0/no-change is equally acceptable as a complete decision.
5. Existing canonical rows/order, explicit g/app/Literal/filter/payload routing, completion/command cache, Macro instantaneous zero/one/many, legacy broker, queryexec, radial/dashboard/hotfolder/programmatic semantics are preserved in the contract. No automatic caller acquires discovery rows.
6. Current typed Actions/args/ActivationSource/ActionSurface remain distinct from proposed query origin; eligibility and selected payload cannot be retargeted by replacement/policy changes. No inferred file/content/action payload or new logging/personalization.
7. Optional proof stays≤128 states,≤30s/run,≤60s aggregate, at most two total invocations with one concrete localized correction. Original evidence is retained; NOT_RUN/failure is honest and nonblocking for a no-change/qualified decision.
8. Exact outcome, counterevidence, unresolved limits and acceptance authority/version are recorded. No F001 execution recommendation follows merely from favorable source classification; missing explicit consumer/compatibility acceptance means conditional/deferred infrastructure.
9. No future goal, production package, merge, release or external write is automatically executed. Complete docs/review can close the spike without accepted implementation.

Acceptance is the complete reviewed decision and evidence within bounds, not a requirement to choose H1 or obtain product approval during the session. If mandatory facts cannot be reconciled, record an explicit incomplete evidence boundary or H0, without fabricating acceptance. Optional model success does not replace source/contract review.

## 7. Fresh branch and checkpoints

After separate user selection, inspect repository/current master/remote relationship, AGENTS, Git status/operations and task ownership. New branch stem **`codex/discovery-admission-decision`** from then-current verified master or explicitly approved integrated tip; preserve dirty work/use isolated worktree as needed. Resolve collisions without overwrite. Resume a confirmed interrupted instance after identity/ancestry checks instead of recreating M0-A. No reset/pull/rebase/force-push or merging historical research is needed.

Execution directory: **`docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/`**. Copy the authorized pair, record historical/current source/base/branch, role settings and section 13 ledger; save evidence/role/proof/review/stalls below it.

| Checkpoint | Objective / owners and outputs | Focused acceptance | Commit subject |
|---|---|---|---|
| M0-A | Safe task bootstrap; execution pair/provenance/role records | Correct source/base/branch, scope/dirty-work ownership, unexecuted downstream packages | `docs(search): record admission decision baseline` |
| M1-A | Bounded source manifest and decision/consumer contract; optional pure model only if needed | Invariants1–8; every required owner classified/Unknown, useful candidate or no-change, exact evidence/budget/acceptance status | `docs(search): decide bounded discovery admission contract` |
| M2-A | Separate focused review and closure; one substantive documentation correction | Invariants1–9 reconciled; truthful accepted/qualified/no-change/incomplete outcome, no source implementation | `docs(search): close admission consumer proof review` |

**M1-A handoff:** objective is the finite H0/H1/H2 decision; owner is execution documentation, not production routing. Facts are the existing snapshot/canonical/activation/settings owners and baseline 81/45/43 limits. Tasks: revalidate current seams; one bounded roster/body pass; at most two candidate contracts; optional proof only for a concrete state ambiguity; write decision and explicit acceptance block. Dependencies: M0-A only. Non-goals/invariants: sections3/6. Verification: static source/reference/contract consistency and optional bounded model, never Cargo/native. Done: exact outcomes/evidence/counterguards, usable candidate or no-change, remaining evidence and acceptance status, coherent commit/push attempt. Deeper grammar or real provider emptiness is an explicit uncertainty, not routine helper work silently expanded.

**M2-A handoff:** separate reviewer reads the contract, manifest, source-critical claims and optional evidence, challenging Complete/default-like reachability, disabled/dynamic owners, automatic consumers, immutable action authority, no-change criteria and unused-infrastructure sequencing. It returns a bounded report; sole writer applies one substantive correction pass and only affected static checks. Do not run a new relevance experiment or broad source/native audit. Done: each required finding accepted/fixed or explicit unresolved disposition; exact final source/commit/push/model/review independence and no-auto-implementation boundary.

Commit coherent checkpoints and attempt one normal push after each. Record failure and retain local commits; no repeated network recovery, unrelated staging, history rewrite or automatic merge.

## 8. Future focused verification — not executed by this proposal

| Risk / behavior | Exact owner/discovery step | Fixture / expected observation | Level and bound |
|---|---|---|---|
| Roster completeness | Reconcile current reload_from_dirs with historical 81-row survey; inspect current trait/manager defaults | Every row has source locator/category or explicit current delta/Unknown; disabled and dynamic policies visible | Static; one bounded source pass,≤256 roster rows/≤45 global entries |
| Global grammar versus content | Named provider entry bodies plus one direct helper layer | Source negative/protected forms and uncertainty stated; deeper unresolved semantics remain Unknown | Static source; no provider invocation |
| Canonical/automatic compatibility | Named gui/search.rs paths, current callers and existing tests as source; source search for candidate insertion seam | No canonical cache/result/cardinality/source-policy change; pending/deferral/stale is not no-match | Static contract trace; no application construction |
| Authority and Settings | Current Action/ActivationSource/ActionSurface and save owners | Proposed origin/binding/acceptance explicitly distinct; no inferred payload/draft-save bypass | Static review |
| Useful candidate and no-change | At most two explicit configuration/query sequences in contract | Source-plausible admission or precise failure; remaining runtime/native gap stated; no synthetic Complete substitution | Document reasoning, no performance/relevance claim |
| Optional state model | Task-local proof/admission_model.py only if concrete need | All declared≤128 states retain guards/identity; unknown/stale/automatic/deferral rejects; original failures preserved | Stdlib pure;≤30s/run,≤60s aggregate; maximum two invocations/one repair; no dependency/native fallback |
| Reproducibility and downstream gate | Exact source/proof/input hashes; contract acceptance block; Goal/Kickstart links | Evidence/version/authority trace; missing acceptance cannot authorize F001 | One initial static document check plus at most one affected check after correction |

No Cargo, rustc, Nextest, Clippy, dependency installation, native/manual/app/benchmark campaign or user-data fixture is required or allowed. Future UI/native proof is a separately accepted consumer's focused obligation, not claimed by this spike. Optional Python absence gets NOT_RUN; no replacement tool installation or Rust fallback. Save sanitized receipts, never private payloads.

## 9. Failure handling, rollback and bounded continuation

One substantive source/document correction pass after review; rerun only affected static checks. Do not convert a newly found opaque owner into a broader framework/recursive audit. Optional proof gets at most one localized correction only within its two-run/60-second total; retain the first output and reason, never retune cases for success. Timeout/failure/exhausted bounds remain visible; if uncertainty undermines H1, choose H0 or explicit incomplete evidence rather than weaken guards.

Continue unaffected documents/review if a role/tool/optional proof is unavailable. At most one supported role-capacity recovery, then truthful authorized fallback; no installation or repeated dispatch loop. A safety/Git conflict stops the affected operation; preserve unrelated work and use a task stall report, not a reset. No passing claim from unrun checks. Revert/supersede only task docs/proof through normal scoped edits; no user data or app migration exists. Any cleanup resolves/checks absolute targets inside the execution directory and removes only task-owned outputs, never another task's files/processes.

## 10. Models and persistent role records

Recommended planner **gpt-6-astra/max**, author **gpt-6-astra/xhigh**, reviewer **gpt-6-astra/max**, parent an available supported Astra xhigh or Sol high. Admission/source authority and useful/no-change judgment warrant high reasoning; optional formatting can use Sol xhigh. If unavailable, one supported recovery then authorized **gpt-6.1-sol/xhigh** fallback with explicit limitations. Verify actual pinned role configuration; prompt names do not override pins. Never change global/sandbox/permission settings to force a model.

Sequential planner→single author→separate reviewer, no overlapping writes. Read-only roles return Markdown for the writer. Under execution `roles/<role>/ROLE_RECORD.md` record requested/configured/resolved-or-unverified model/effort, role ID, source/base, input/output, authorship/persistence and actual review independence. Store planner/author ANALYSIS and reviewer REVIEW separately; parent self-review is not independent. Source/configuration verification is not a serving-backend receipt.

## 11. Progress and stall reports

Maintain execution `GOAL.md` section 13 at real transitions: branch/base/worktree, current/source manifest identity, checkpoint/outcome, evidence, optional state/run budget, current issue, exact next action, commits and actual push visibility. A genuine pause/block/defer uses `stalls/<checkpoint>-<UTC>.md` below this task directory: sanitized command/diagnostic, expected/actual, preserved changes, ownership/cleanup, remaining bounds, source/evidence and smallest safe resume action. Generate timestamps at execution. Publish a truthful partial/no-change report when safe; no acceptance claim from a stall commit or unresolved index conflict.

## 12. Final delivery and authorization boundary

Deliver execution Goal/Kickstart, SOURCE_ADMISSION_MANIFEST.md, CONSUMER_COMPATIBILITY_CONTRACT.md, ADMISSION_DECISION.md with H0/H1/H2 and separate acceptance block, optional proof/receipts or exact not-run reason, role/review/progress/stall records and scoped verification. Report historical/current source, base/branch/checkpoints/final commit, push success/local-only failure, bounded coverage/unknowns, optional runs/count/time, source-versus-runtime/native limits, final reviewer disposition and no-change/qualified outcome.

**No metadata/ranker/UI implementation is delivered or automatically authorized.** F001 requires explicitly accepted concrete consumer/compatibility contract version/F002 closing commit **and separate user selection/authorization**; favorable source classification or this spike's own successful completion is insufficient. Negative/inconclusive/not-accepted outcomes retain no production change. No generated future goal, PR/merge, release/deployment/destructive data/external account action executes automatically. This pair itself is proposed only.

## 13. Execution progress — mutable

State: **PROPOSED_NOT_AUTHORIZED**. No future branch, source pass, optional model invocation, commit, consumer acceptance or independent spike review has occurred.

| Checkpoint | State | Evidence / next boundary |
|---|---|---|
| M0-A | pending | Await separate user selection; inspect current master and task ownership |
| M1-A | pending | No current-source classification/consumer decision or optional proof |
| M2-A | pending | No review/closing commit or accepted consumer contract |

Next action: user separately selects this bounded spike, then M0-A. Do not execute F001 or another roadmap item to establish this package's prerequisites.
