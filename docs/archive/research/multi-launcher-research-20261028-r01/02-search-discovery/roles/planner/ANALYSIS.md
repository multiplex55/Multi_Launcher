# Goal 02 — bounded architecture and checkpoint handoff

**Author:** `/root/audit_planner`, 2026-10-28. Requested/configured Astra max; resolved backend/effort unverified. Read-only planning; no prototype, application or test execution. All application-source locators below refer to commit **`59156d96adcc73939c7171487a0cb582e3286587`**. Research branch/base/baseline are recorded in [ROLE_RECORD.md](ROLE_RECORD.md).

## Decision and limits

Proceed with deterministic, local **capability discovery for the interactive main launcher**, while leaving canonical query resolution and execution authority intact. The initial feature boundary is a separate interactive presentation layer, eligible only after fresh user text input in `ActionSurface::LauncherList` or `LauncherGrid`. It must not append rows to the shared `search()` result vector, plugin command cache, completion index, saved-query resolver or Macro result cardinality.

This is a topology decision for the research author, not authorization to implement production integration. The only accepted actionable planner proposal is **SEARCH-F001**, a small typed metadata foundation using existing plugin identity/enablement snapshots and existing safe tool-opening actions. It is independently useful preparation with anticipated maintainability value, not a demonstrated relevance gain or a reproduced defect. Its complete pair is in this role's `follow-on/SEARCH-F001-built-in-discovery-metadata/` directory. No other actionable planner package is accepted at this point.

Read the closed [audit search handoff](../../../01-static-audit/SEARCH_HANDOFF.md), [review and limitations](../../../01-static-audit/REVIEW_AND_LIMITATIONS.md) and [follow-on index](../../../01-static-audit/FOLLOW_ON_INDEX.md) before interpreting any older planning notes. Raw Goal 01 role records remain frozen.

## Source-backed ownership map

| Pinned source locator | Relevant fact and consequence |
|---|---|
| [`src/actions/mod.rs:12–19`](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/actions/mod.rs#L12-L19), `Action` | The executable contract contains label, description, `action` and optional `args`. Preserve the exact action/args pair; a display label or discovery ID is not execution authority. |
| [`src/plugin.rs:382–422`](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L382-L422), `PluginSlot`, `PluginCatalogOrigin`, `PluginCatalogSnapshot`; `1031–1079`, registration; `1108–1159`, revisioned snapshot | Existing manager ownership already supplies immutable name/origin/epoch/enablement/declared-capability facts without provider locks or private settings. Reuse it. Runtime enabled-capability restrictions are a separate input and are not supplied by the declared-capability list alone. |
| `src/settings_editor/catalog.rs:272–318`, `SettingsCatalog::from_plugin_snapshot` | Existing pure metadata projection detects ambiguous plugin names and does not let a dynamic plugin inherit builtin annotations by name. This is a useful ownership pattern; Settings search itself is a different catalog and must remain intact. |
| `src/plugin.rs:1174–1185`, `commands_filtered`; `src/gui/search.rs:707–767`, `update_command_cache` and completion rebuild | Commands are provider-produced shortcuts; command cache feeds empty browse, fallback and completion. Injecting new discovery commands here would affect consumers before any interactive gating. Preserve lists and their semantics. |
| [`src/gui/search.rs:1125–1392`](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1125-L1392), provider scoring; `1398–1470`, ordinary, deferred and failed-provider paths; `1500–1667`, final composition | These are canonical route/score/lifecycle owners, shared with read-only consumers. Pending state, provider deferral and final row count are separate facts. New discovery scoring must not quietly replace their policy. |
| [`src/gui/render.rs:3095–3191`](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L3095-L3191), `handle_macro_launcher_command`, production and compatibility broker polling | Production Macro calls `search()` and immediately interprets zero/one/many rows; it does not wait for pending to settle. The compatibility broker chooses the first row with Enter source. Adding a discovery row could turn a formerly empty query into execution, a unique query into a chooser, or a multirow query into a different first activation. Keeping shared vectors unchanged avoids all three. |
| [`src/universal_actions/model.rs:104–145`](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/universal_actions/model.rs#L104-L145), `ActionSurface`, `UniversalActionInvocationContext`; `src/commands/model.rs:14–36`, `ActivationSource` | UI surface and triggering input are distinct. Main-launcher discovery means list/grid presentation, not a new invocation source. Click/Enter must retain their actual source and use the existing execution path. “MainLauncher” and “CommandInvocationSource” are not current source symbols. |
| `src/plugin.rs:1310–1327`, `has_enabled_query_prefix`; `1364–1386`, `plugin_query_plan`; `1411–1437`, `plugin_matches_search` | Existing prefix classification respects Literal policy and enabled/capability gates, but `has_enabled_query_prefix` only recognizes enabled searchable providers and takes provider read locks. It is neither a complete reserved-syntax registry nor a guaranteed lock-free admission API. Disabled known syntax must not become natural-language discovery by accident. |
| `src/plugins_builtin.rs:10–43`, web; `58–110`, calculator; `src/plugins/omni_search.rs:129–165`, Omni | Explicit `g`, `=` and `o` behavior already exists. Calculator search can append history under its settings. Never build discovery metadata by invoking arbitrary `Plugin::search`, even when only “inspecting” results. Omni already owns combined content search; this proposal is not a replacement content engine. |
| `src/usage.rs:11–34`, `UsageEntry`/`load_usage`; `src/gui/search.rs:1500–1667`, final ranking | Existing usage is action-string/count data, without recency timestamps. It is not permission, stable discovery identity or a time-of-day signal. Research uses synthetic counts only. |

The closed audit remains authoritative for deeper Universal Actions, HotFolder, radial lease, input/focus and privacy traces. The planner has not reopened the entire audit.

### Bounded real opener seed

These are **existing typed tool-opening actions**, all with `args: None`. They are not inferred execution on the text following a natural-language query.

| Owner and pinned source lines | Exact action(s) | Bounded meaning |
|---|---|---|
| `src/plugins/json_utility.rs:8–55`, `JsonUtilityPlugin::search`; `69–82`, shortcuts | `json_utility:open`, `json_utility:format`, `json_utility:minify` | Open the existing general/format/minify mode. `json pretty` is an alias for the existing format intent, not a fourth capability. |
| `src/plugins/hash_generator.rs:6–41` | `hash:open` | Open Hash Generator; no implicit path or clipboard load. |
| `src/plugins/xml_toolkit.rs:6–41` | `xml:open` | Open existing bounded local XML tool. |
| `src/plugins/regex_tester.rs:6–41` | `regex:open` | Open Regex Tester, not execute inferred arbitrary pattern/input. |
| `src/plugins/table_utility.rs:6–41`; `src/plugins/csv.rs:6–41` | `table:open`, `csv:open` | Open the existing Table/CSV profile. Do not advertise full-file export from a bounded preview. |
| `src/plugins/image_compare.rs:6–41` | `imgdiff:open` | Open exact image comparison; no perceptual/AI matching claim. |
| `src/plugins/duplicate_finder.rs:6–41` | `dff:open` | Open read-only duplicate discovery over subsequently selected folders; no automatic deletion. |
| `src/plugins/activity.rs:6–41` | `activity:open` | Open bounded background activity; this is not a Macro run-history UI. |

This is eleven fixed opener descriptors across nine plugin owners, deliberately small enough to implement without a general plugin metadata migration. `src/commands/parser.rs:380–460,498–538,943–950`, the existing `handlers/{json_utility,xml_toolkit,hash_generator,table_utility,dialog_crop}.rs`, and `src/gui/command_host.rs:218–269,822–839` connect these payloads to typed commands and existing dialogs. Most `commands()` entries are instead `query:...` shortcuts; preserve that distinction exactly.

The author may include further source-backed catalog entries without broadening SEARCH-F001. For example, `src/plugins/file_search.rs:33–72,661–731` separates safe `open_action()` from mode/start actions; `src/plugins/diff.rs:14–30,43–60,107–122` separates an empty Diff opener from supplied paths. Inspect their actual typed payloads before authoring those entries. ZIP extraction, automatic duplicate deletion, Macro run-history browsing and complete export of unmaterialized file contents are not established by these tool names. Include unsupported queries as negative cases rather than invented capabilities.

## Non-negotiable compatibility ledger

1. Empty/whitespace browse remains command cache plus catalog actions with `app ` display labels; the original action/args remain unchanged. Unprefixed natural text is not automatically a local-app search; local app scoring belongs to `app`/`app `.
2. Non-`g` provider input vector **already empty** can trigger command-cache fallback. `g ` returns before fallback. A nonempty provider vector emptied by the later exact post-filter does **not** fall back again.
3. Capacity deferral passes no provider scores directly to final composition. It can retain local `app` rows; other nonempty routes can remain row-empty. It does **not** invoke command-cache fallback. Failed-provider reopening is separate and can invoke the non-`g` fallback through `search_plugins_from_results`.
4. Preserve the explicit `g ` provider-selection exception and its capability checks. Do not describe it as ordinary enabled-provider filtering. Preserve Literal policy only when the original head matches the plugin's prefix.
5. The snippet no-tag byte-preservation observation is helper-local. Upstream ActionFilters may already have transformed the query. The terminal escape defect is already **AUD-F001**, not another search recommendation or a claim that all Windows paths/plugins are corrupt.
6. Pending may coexist with rows, and Macro's current cardinality decision is instantaneous. Discovery must not add a wait, change success meaning, change the legacy broker or silently alter these contracts.
7. Existing scoring adds action-ID usage counts and has no explicit semantic stable tie key. Do not claim the current ranker is deterministic by stable identity, or that a Python implementation reproduces all current providers/fuzzy behavior.
8. Keep shared command parsing, dispatch, Universal Actions, confirmation and selected exact Action+args. A relevance score cannot grant permission, manufacture a payload, bypass a disabled plugin or replace target identity checks.
9. Normal root activation does not universally revalidate provider provenance. Any stronger discovery-row guard is a deliberate future integration requirement, not an existing guarantee.
10. Keep current Jump Mode, modal/focus and fresh-Enter fencing. No synthetic input, arbitrary delays, new hotkeys or native focus workarounds are implied.

## Runtime design boundary the author must preserve

The proposed flow is: current interactive query state → protected-route/admission decision → immutable eligible builtin metadata → pure discovery ranking → a separate presentation collection → an explicitly selected exact Action → existing activation/command authority.

The canonical `LauncherSearchOutcome.actions`, `self.results`, command cache and completion index remain the inputs used by existing automatic consumers. A later GUI integration may display a separate discovery section alongside canonical rows, but must not repurpose those shared vectors as its storage. A typed presentation-row origin and a **proposed** query-origin discriminator distinguish fresh user editing from Macro/programmatic/saved-query/radial/dashboard/hotfolder work. Programmatic text shown in the main launcher does not become discovery-eligible merely because it is visible there. A fresh user edit is the explicit transition to interactive admission.

Initial admission is main-launcher list/grid only, feature opt-in, bounded natural-language text, no known explicit prefix or other protected syntax, no raw path/URL/shell/calculator/filter interpretation, no provider pending/deferral or unstable catalog, and **no canonical results**. This conservative no-results policy may miss useful discovery when existing providers produce low-score rows; quantify that limitation rather than weakening exact-route preservation to improve synthetic numbers. A disabled known prefix remains reserved. Dynamic-prefix inventory completeness is an integration prerequisite, not solved by the eleven builtin descriptors or `has_enabled_query_prefix` alone. Until a future package specifies a revisioned complete reserved-route view, unknown admission fails closed.

Within discovery, eligibility precedes ranking. Missing, ambiguous, disabled, unsupported or stale owners produce no executable recommendation. Tool opening is distinct from operations on supplied data: no natural-language tail is injected into `args`, query override, shell text, clipboard content or a path payload. Ambiguous input yields a chooser or abstention; destructive/unsupported instructions do not gain an executable guessed operation. Later ordinary actions offered for a selected existing result continue through Universal Actions.

Freeze each visible row to descriptor ID, provider origin/name/epoch, metadata/policy version, query generation and exact Action+args. A changed selection target is invalidated before activation; no row-index retargeting after rerank. On activation, revalidate current owner/eligibility and preserve actual `ActivationSource` and `ActionSurface`. Do not promise this guard already exists for every legacy row. No new worker is needed for an eleven-entry local catalog; if future scale warrants a worker, generation and retirement ownership must be designed rather than borrowed from unrelated provider lifecycles.

## Ordered research-author packets

All packets are documentation/synthetic research only. No production edits, Cargo/rustc/Nextest/Clippy, app/native runs, dependencies or private-data ingestion. Source inspection is permitted only to resolve the named handoff. Parent owns branch/checkpoint commits and one normal push attempt after each accepted checkpoint; failed pushes preserve local progress. Author is the sole writer. Complete the prescribed stages without introducing extra planning stages.

### M1-A — current architecture and real capability catalog

- **Objective:** Write `CURRENT_SEARCH_ARCHITECTURE.md`, `COMMAND_AND_CAPABILITY_CATALOG.md` and `catalog/catalog.json` with a traceable current/proposed distinction.
- **Ownership/current facts:** Canonical search is `gui/search.rs`; manager owns provider identity/metadata; plugin owners produce Actions; commands own parsing/dispatch. Use the source map and corrected compatibility ledger above, not generic launcher assumptions.
- **Scope:** Named source paths above and directly needed typed handlers/tests. Expand only enough real tools to support the evaluation's intended capabilities. Record exact shortcuts separately from exact opener payloads, declared versus enabled capabilities, object/action/tool categories, aliases, non-goals and evidence lines.
- **Required steps:** Map query→filters/provider/local path→scores/pending→results→activation; enumerate automatic consumers and explicit route exceptions; produce stable synthetic catalog IDs and exact wire payloads; explain safe opening versus data operation; record unavailable/unsupported meanings and coverage gaps. Catalog construction must not invoke providers or read user stores.
- **Invariants/non-goals:** Preserve Macro cardinality and current shortcut sets. No live-data inventory, exhaustive plugin audit, schema migration or invented feature. No implicit ZIP/run-history/full-export entries.
- **Dependencies:** M0-A and closed Goal 01 evidence. SEARCH-F001 production execution is not required; JSON catalog is research data, not a shipping registry.
- **Tests/verification:** Static link/symbol/payload reconciliation and JSON schema/ID/owner completeness checks using standard text/JSON tools only. Mark unsupported entries explicitly. No actual action execution.
- **Done:** Every included capability has a real source owner, valid exact action contract or explicit non-executable status; canonical route matrix includes all corrected fallback/pending distinctions; catalog source and gaps are documented.
- **Genuine uncertainty:** Which additional owners are needed for meaningful catalog coverage is the author's bounded source decision; the eleven seed mappings and ownership topology are settled.

### M1-B — precise grammar, ranking and integration design

- **Objective:** Produce `INTENT_TAXONOMY_AND_GRAMMAR.md`, `RANKING_SPECIFICATION.md` and `RUNTIME_AND_INTEGRATION_DESIGN.md` detailed enough for a later pure Rust implementation.
- **Ownership/current facts:** Pure discovery is a new projection/ranker over existing metadata. It is not a `Plugin::search` replacement, command dispatcher or change to `search_read_only_outcome`.
- **Scope/required steps:** Specify route precedence and rejection conditions first; distinguish exact protected routing from natural text, typo handling and ambiguous intent; define tokenization/normalization, synonym ownership, weighted fields, bounded typo candidates, candidate-generation limits, exact numeric score formulas, thresholds, deterministic tie tuple, top-k, abstention/chooser margins, query length bounds and cache invalidation. Define lifecycle/admission and stable selected payloads using the boundary above. Keep legacy and discovery score scales separate. Identify complete-prefix inventory as an integration prerequisite.
- **Candidate budget:** Predeclare at most three policies total: **P0** a transparent label/shortcut lexical surrogate; **P1** curated synonyms with deterministic token matching; **P2** intent/object-aware lexical scoring plus bounded typo evidence. P0 is a documented-policy surrogate, not the actual launcher baseline. Same catalog, gates and identities across policies. The author chooses exact formulas now, not after looking at held-out errors.
- **Usage:** Prefer zero usage weight for the initial shipping proposal. If evaluated, use a predeclared bounded synthetic count transform inside the same policies; show cold start, heavy-use perturbation and disabled usage. No recency/time-of-day claim or raw history input. Never let usage override eligibility, protected routing or abstention safety.
- **Invariants/non-goals:** Main launcher list/grid only; no radial extension; canonical results and Macro zero/one/many remain unchanged; no LLM, workspace/scope bar, new persistence or general provider/dispatcher rewrite.
- **Dependencies:** Accepted M1-A catalog. No required source implementation.
- **Tests/verification:** Hand-work several score decompositions, ties, typo cutoff, no-match and ambiguous examples; static consistency checks that every score term/constant and policy is defined. Design a future source/epoch disable-before-Enter test and zero/one/many Macro parity cases, but do not run them.
- **Done:** A new implementer can reproduce candidate order and abstention from definitions alone; all hard gates, ownership, invalidation and excluded consumers are explicit; candidate configs are ready to freeze.
- **Genuine uncertainty:** Current source does not provide the proposed complete nonblocking reserved-syntax view. Record the precise future integration gap and fail-closed policy; do not silently claim F001 solves it.

### M1-C — UX, safety, external comparison and evaluation freeze

- **Objective:** Finish `KEYBOARD_AND_RESULTS_UX.md`, `PRIVACY_SAFETY_AND_COMPATIBILITY.md`, `ALTERNATIVES_AND_EXTERNAL_REFERENCES.md`, initial `DECISION_RECORDS.md`, `evaluation/DATASET_README.md`, `queries.jsonl` and `EVALUATION_PROTOCOL.md`; freeze inputs before running candidate comparisons.
- **Ownership/current facts:** Existing input/focus and Universal Actions remain authoritative. Main-launcher discovery is a separately labeled tool-opening section; current auto-selection behavior must not become guessed operation execution.
- **Required steps:** Specify no-results/disabled/ambiguous/pending/too-long responses, explicit row selection and Enter, query edits and selection invalidation, availability reasons, and the exact transition from programmatic to interactive origin. State that data is neither loaded nor transformed from query wording. Incorporate the parent's four primary-source comparisons from `roles/orchestrator/EXTERNAL_EVIDENCE.md` with narrow citations; they supply interaction precedents, not measured quality.
- **Dataset:** Target about **300**, minimum **250** meaningful cases. A reasonable starting allocation is 70 protected routes, 80 natural discovery, 40 typos, 35 ambiguity, 30 availability/context, 25 unsafe/unsupported and 20 edges; revise counts only with a coverage reason. Avoid paraphrase padding. Each case carries stable ID, family/cluster, synthetic context/settings/provider state, expected admission/route, graded acceptable IDs, forbidden results, abstention/chooser expectation, source references and label rationale/confidence.
- **Split/freeze:** Group by semantic/template family, approximately 60/20/20 development/validation/holdout; keep near-duplicate templates in one split and reserve some ambiguity/unsafe families. Document author-visible labels explicitly: this is **not blind evaluation**. Freeze catalog, case file, split membership, policies/constants and protocol with content hashes before M2-A. Record edits after freeze as versioned deviations. Do not choose cases to make P2 win.
- **Metrics:** Separate route-preservation/eligibility/admission denominators from top-1/top-k/MRR or nDCG; add ambiguity, abstention, forbidden suggestions, disabled-owner suggestion rate, stable ties and synthetic usage sensitivity. Conditional discovery quality must state exclusions; cases gated because canonical results exist must not disappear from overall coverage reporting. Performance is Python surrogate resource observation, never Rust/UI latency.
- **Dependencies:** M1-A/B. No runtime implementation required.
- **Tests/verification:** Validate IDs, family split disjointness, source coverage and label consistency without executing launcher code. Manually inspect representative adversarial labels. No private input, subprocess or provider invocation.
- **Done:** Frozen hashes/protocol, bounded candidate/run plan and honest leakage limitations exist; safety outcomes are separate from relevance and all catalog entries have meaningful cases.
- **Genuine uncertainty:** Synthetic labels cannot establish actual user preference or live-provider fidelity. Keep that limitation explicit regardless of scores.

### M2-A — isolated reference prototype and bounded comparison

- **Objective:** Implement only the allowed standard-library Python prototype under `prototype/`, with reproducible outputs under `evaluation/`.
- **Ownership:** The author owns a synthetic reference harness, not Rust production code. The planner has not executed or pre-implemented it.
- **Required steps:** Read frozen catalog/cases/configs; implement the specified pure gates/rankers and stable identities; emit per-case admission/order/score explanation/violations and aggregate metrics; add a small meaningful smoke script for gates/ties/abstention; run the frozen policies within the budget. No shell, network, provider, clipboard, OS or app calls from the prototype; files limited to the research output area.
- **Invariants/non-goals:** At most three predeclared policies including P0. No grid search, training pipeline, live corpus, runtime plugin parity claim or production latency claim.
- **Dependencies:** M1-C freeze. If Python/tool availability blocks execution, preserve all artifacts with `NOT_RUN` and a precise limitation; do not install dependencies or stop unrelated documentation work.
- **Tests/verification:** One initial evaluation per candidate, then at most one evidence-backed correction-and-reevaluation cycle. Each invocation ≤120 seconds; aggregate evaluator runtime ≤10 minutes. Record actual commands, versions, hashes, elapsed time, failures and corrections. Waits must permit progress communication.
- **Done:** Reproducible raw outputs and budget ledger exist, or exact `NOT_RUN` evidence exists. No silent post-freeze tuning or overwritten failure record.
- **Genuine uncertainty:** A bug in the reference implementation is a bounded correction, not evidence the concept or launcher is broken.

### M2-B — interpret results and make bounded decisions

- **Objective:** Produce `evaluation/RESULTS.md`, `EVALUATED_EXAMPLES.md` and final ranking/integration decision records grounded in actual outputs or explicit nonexecution.
- **Required steps:** Report numerators/denominators and per-family failures; show wins, regressions, ambiguous labels, protected routes and forbidden outcomes; compare identical candidate inputs; distinguish exact gate checks from semantically judged labels. Examine leakage and selection sensitivity. Choose one candidate, a narrower scope or no ranking change based on the frozen criteria; no quota requires improvement.
- **Invariants/non-goals:** No claim of measured launcher relevance or latency; no hiding excluded/provider-pending cases; no expanding candidates or retuning a holdout after an unfavorable result.
- **Dependencies:** M2-A artifacts and protocol; `NOT_RUN` supports a design-only recommendation with limitations, not fabricated results.
- **Tests/verification:** Reconcile aggregate figures to per-case records, sample each major failure family and verify file hashes. Repeat execution only if the single permitted correction cycle remains and a concrete harness defect warrants it.
- **Done:** Every quantitative claim links to reproducible evidence; per-family risk and unmeasured integration gaps are explicit; ranking choice/no-change is justified without external-product performance claims.
- **Genuine uncertainty:** Real-user workload and native behavior remain future evidence, not solvable by more synthetic paraphrases.

### M3-A — actionable packages and cross-goal synthesis

- **Objective:** Complete `RECOMMENDATIONS.md`, `FOLLOW_ON_INDEX.md`, all accepted Goal/Kickstart pairs, cross-goal synthesis and the review handoff.
- **Required steps:** Promote or precisely amend SEARCH-F001 after checking source/catalog alignment. Package any newly accepted pure-ranker or opt-in-UI recommendation with the full templates and dependencies; otherwise leave it explicitly unaccepted. A future UI package must solve complete reserved-route metadata, user-origin admission, immutable selection, live eligibility and automatic-consumer exclusion before it is ready. Link **AUD-F001** for terminal escape preservation instead of creating a duplicate; keep **AUD-F002** independent. Do not make either audit package a prerequisite without a concrete new dependency.
- **Invariants/non-goals:** Research packages remain `PROPOSED_NOT_AUTHORIZED`; no production implementation. Do not write vague tickets that require another architectural discovery session, or copy raw closed audit proposals over canonical corrections.
- **Dependencies:** Prior research artifacts and factual reconciliation. No acceptance quota; ranking/UI can remain design-only if evidence or packaging is incomplete.
- **Tests/verification:** Check link/ID consistency, Goal/Kickstart agreement, source/base provenance, dependency ordering, exact future verification budgets, rollback and model truthfulness. No Cargo/native checks.
- **Done:** Every accepted actionable recommendation has one canonical complete pair or precise existing-pair amendment; unsupported ideas are visibly unaccepted; parent receives a reviewable research tree with limitations. M3-B independent review/closure belongs to the parent and reviewer.
- **Genuine uncertainty:** Independent review may reject a claim or package. Preserve the raw contribution and record canonical corrections rather than rewriting authorship history.

## Recommendation disposition

| ID/topic | Planner disposition | Dependency/next action |
|---|---|---|
| SEARCH-F001 builtin discovery metadata | Accepted planner proposal; complete pair returned; `PROPOSED_NOT_AUTHORIZED` | Independent future foundation. Reuse manager metadata and plugin-owned fixed opener constructors. No query/UI integration. |
| Pure discovery ranker | Unaccepted design target | Requires M1-B exact definitions and M2 evidence/limitations, plus a full package if adopted. |
| Opt-in main-launcher discovery integration | Unaccepted design target | Requires origin/admission/complete-prefix/selection proof and a full package. Must preserve canonical search and automatic consumers. |
| Usage personalization, dynamic-plugin discovery, radial discovery | Unaccepted/outside initial scope | No new policy or implementation package proposed here. |
| Terminal raw escape | Existing AUD-F001 | Link the closed canonical audit pair; no duplicate parser package. |
| Diff file-operation authority | Existing AUD-F002 | Separate bounded spike; not a discovery prerequisite. |

Planning stops here. The author has concrete owners, safe seeds, admission constraints, stage outputs and evaluation limits. Further catalog expansion, numeric design and prototype work belong to the authorized research checkpoints, not more planner exploration.
