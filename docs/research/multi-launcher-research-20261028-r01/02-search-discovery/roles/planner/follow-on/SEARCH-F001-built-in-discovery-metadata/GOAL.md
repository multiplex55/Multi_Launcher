# Goal SEARCH-F001 — Builtin tool discovery metadata with existing action ownership

## 0. Identity, provenance and readiness

- **Package/recommendation:** SEARCH-F001.
- **Source program:** Multi Launcher research 2026-10-28-r01, Goal 02 search/action discovery.
- **Canonical proposed directory:** `docs/research/multi-launcher-research-20261028-r01/02-search-discovery/follow-on/SEARCH-F001-built-in-discovery-metadata/`.
- **Raw author proposal:** `roles/planner/follow-on/SEARCH-F001-built-in-discovery-metadata/` under the same research goal.
- **Type/state:** feature foundation with behavior-preserving extraction; `PROPOSED_NOT_AUTHORIZED`.
- **Readiness:** `READY_FOR_IMPLEMENTATION_REVIEW`. The architecture is bounded; authorization and current-source revalidation precede implementation.
- **Priority/value:** medium roadmap priority. It provides one testable representation of real tool-opening capabilities for later local discovery. User-efficiency benefit is anticipated, not measured, and this package itself adds no search UI.
- **Severity:** not a demonstrated defect. Existing command and Settings catalogs already work; this supplies a different narrow metadata purpose.
- **Confidence/complexity/risk:** high confidence in the named source seams; moderate implementation complexity; low-to-moderate regression risk from extracting existing Action construction; moderate targeted validation burden. Production relevance and UX benefits remain unmeasured.
- **Source evidence:** `59156d96adcc73939c7171487a0cb582e3286587`. This is a historical evidence pin, not the required future checkout.
- **Authorship:** `/root/audit_planner`, 2026-10-28, requested/configured `gpt-6-astra`/`max`; serving model/effort unverified. Parent persists the returned proposal. No independent reviewer has approved this raw pair.
- **Source finding/design:** Goal 02 `roles/planner/ANALYSIS.md`, sections “Source-backed ownership map,” “Bounded real opener seed” and “Runtime design boundary.” The parent may promote this pair with recorded corrections after research review.
- **Related work:** closed Goal 01 `SEARCH_HANDOFF.md` and canonical `AUD-F001`/`AUD-F002` packages. Neither is a prerequisite; do not duplicate or execute either here. Other search ranking/UI proposals remain separate and unaccepted until packaged.
- **Companion:** [KICKSTART.md](KICKSTART.md).

## 1. Objective and observable outcome

Add a small internal, immutable, typed catalog for **eleven existing payload-free tool-opening actions across nine builtin plugins**. Derive executable Actions from constructors owned by those plugins. Bind catalog availability to the existing manager's plugin origin/name/epoch and current enablement/capability policy. Building or querying the catalog must not call provider search, read private data, start work or dispatch an action.

For example, the new catalog can describe “format JSON” as the existing `json_utility:format` opening intent. The existing `json format` query still yields the same Action, and the existing `query:json format` shortcut remains unchanged. Natural-language queries will not start finding that descriptor until a separately authorized integration package exists.

All current user-visible behavior remains identical: explicit routes, labels, descriptions, action/args, command cache cardinality/order, completion, empty browse, fallback, Macro cardinality, dialogs and Universal Actions. The engineering gain is a bounded, source-owned discovery vocabulary and immutable eligibility/provenance contract. There is no performance-improvement claim.

## 2. Current architecture and evidence

All line references in this section are pinned to [source commit 59156d96adcc73939c7171487a0cb582e3286587](https://github.com/multiplex55/Multi_Launcher/tree/59156d96adcc73939c7171487a0cb582e3286587).

| Owner/path/symbol | Current responsibility | Package change | Relevant contract |
|---|---|---|---|
| `src/actions/mod.rs:12–19`, `Action` | Existing label/description/action/optional-args contract | Reuse unchanged | Preserve exact action+args and existing displayed fields. |
| `src/plugin.rs:382–422`, `PluginSlot`, `PluginCatalogEntry`, `PluginCatalogSnapshot`; `1031–1079`, registration; `1108–1159`, metadata snapshot | Cached plugin identity/origin/epoch, enabled state and declared capabilities; revisioned snapshot without provider locks | Consume this existing internal snapshot; no new provider registry | Declared capabilities are not the separately configured enabled-capability map. Builtin annotations must not be inherited by a same-named dynamic plugin. |
| `src/settings_editor/catalog.rs:272–318`, `SettingsCatalog::from_plugin_snapshot` | Existing separate Settings metadata projection and duplicate-name protection | Reuse the pattern, not its UI catalog or field IDs | Leave Settings catalog behavior and source owner intact. |
| `src/plugins/json_utility.rs:8–55,69–82` | JSON intent parsing and fixed opening Actions; query shortcuts | Extract a pure crate-visible opening-Action constructor by `JsonUtilityIntent`; reuse it from current search and new metadata materialization | Preserve General/Format/Minify and `pretty`→Format; do not change command shortcuts. |
| `src/plugins/{hash_generator,xml_toolkit,regex_tester}.rs:6–41`; `src/plugins/{table_utility,csv,image_compare,duplicate_finder,activity}.rs:6–41` | Each owns a fixed opener and separate query shortcut | Extract/reuse a pure crate-visible fixed opener constructor per owner | Constructors accept no query/path/content parameter; existing `search` branches retain exact matching and returned Action fields. |
| `src/commands/parser.rs:380–460,498–538,943–950`; `src/commands/handlers/{json_utility,xml_toolkit,hash_generator,table_utility,dialog_crop}.rs`; `src/gui/command_host.rs:218–269,822–839` | Typed parse→handler→existing dialog opening | Read-only compatibility evidence; no new command variants or dispatch | Opener materialization is not execution. Preserve modes/profile and args rejection. |
| `src/gui/search.rs:707–767,1125–1667`; `src/gui/render.rs:3095–3191` | Shared command/completion/search and Macro automatic behavior | No production edits or catalog integration here | No new rows, command shortcuts, score policy, pending semantics or zero/one/many changes. |
| `src/lib.rs:19–70` | Crate module declarations | Add one crate-private `search_discovery` module | No new crate, Cargo dependency, feature or serialized schema. |

Counterevidence matters: the repository already has command shortcuts, typed commands, a manager metadata snapshot, Settings search and Universal Actions. This package does not recreate those systems. `ActionIdLookup` is not a sufficient discovery binding because its key is the action string and its catalog behavior is separate from provider-origin/epoch identity. A dedicated small discovery ID expresses a different metadata identity without changing existing action IDs or usage keys.

The current `has_enabled_query_prefix` at `src/plugin.rs:1310–1327` consults enabled searchable providers and can acquire provider read locks. It does not supply a complete reserved-syntax snapshot. This package does not solve admission for all dynamic/disabled routes; a later UI integration must do so before exposing discovery.

## 3. Scope, non-goals and dependencies

Allowed production paths are a new `src/search_discovery/` module, its crate-private declaration in `src/lib.rs`, and the nine plugin files named above for pure Action constructor extraction and directly relevant tests. Add narrow pure tests under the new module. Existing `src/plugin.rs` metadata APIs and inert test seams may be consumed without changing their production semantics; a localized test-only helper adjustment is allowed if genuinely required. Documentation lives in the execution directory specified below.

The initial descriptor set is fixed:

| Stable proposed discovery ID | Builtin plugin name | Existing exact action | Args |
|---|---|---|---|
| `tool.json.open` | `json_utility` | `json_utility:open` | None |
| `tool.json.format` | `json_utility` | `json_utility:format` | None |
| `tool.json.minify` | `json_utility` | `json_utility:minify` | None |
| `tool.hash.open` | `hash_generator` | `hash:open` | None |
| `tool.xml.open` | `xml_toolkit` | `xml:open` | None |
| `tool.regex.open` | `regex_tester` | `regex:open` | None |
| `tool.table.open` | `table_utility` | `table:open` | None |
| `tool.csv.open` | `csv` | `csv:open` | None |
| `tool.image_compare.open` | `image_compare` | `imgdiff:open` | None |
| `tool.duplicates.open` | `duplicate_finder` | `dff:open` | None |
| `tool.activity.open` | `activity` | `activity:open` | None |

These IDs are new internal discovery IDs, not migrations of existing commands, Universal Action IDs or persisted data. Confirm each plugin `name()` against current source before editing; change the package record if the implementation has evolved.

Non-goals: ranking/tokenization/fuzzy algorithms; UI integration or opt-in settings; command/cache/completion changes; live provider enumeration through `search()` or `commands()`; plugin trait/ABI changes; dynamic-plugin discovery; all-prefix admission; radial/dashboard/hotfolder/Macro changes; action dispatch; inferred payload execution; usage/history collection; content/file/clipboard indexing; additional tool descriptors; XML/JSON/table engine changes; parser escape repair; native input changes; global config or dependency changes. Do not advertise ZIP extraction, Macro run-history browsing, automatic duplicate deletion or full-file export as capabilities of these entries.

This is independent future work from the **then-current verified master**, unless the user explicitly selects another integrated base. It does not require execution of the research branch or any unaccepted ranking/UI package. If equivalent typed metadata already exists, record the source-backed no-op or remaining delta rather than creating another catalog. Revalidate named constructors and metadata semantics on the actual base.

## 4. Proposed design

### Ownership and interface

`search_discovery` owns immutable descriptive vocabulary and projection policy for this narrow builtin set. Each plugin remains the owner of its executable opening Action. `PluginManager` remains the owner of provider registration, origin, lifetime identity and enabled runtime state. The caller supplies the current configured enabled-provider set and enabled-capability map as borrowed policy inputs; the new module must not own another mutable permissions store.

Use a finite typed discovery ID and a fixed opener-kind representation, with stable textual IDs from the table. Each descriptor contains a user-facing label, short capability description, bounded curated aliases/intent/object terms, exact explicit shortcut examples, an **open-existing-tool** operation class, expected builtin owner, and provider binding when available. Metadata describes capability, not a user payload. Include an explicit limitation or scope description where the label could overpromise, especially Activity, duplicate discovery, exact image comparison and bounded Table/CSV behavior.

A pure projection accepts `PluginCatalogSnapshot` plus the borrowed current search policy and returns an immutable catalog with availability reasons and exact eligible bindings. A pure resolution operation accepts a discovery ID and expected binding and returns the fixed Action only if that binding is still current and eligible. Illustrative contract:

```text
project(existing_plugin_snapshot, current_enabled_policy) -> immutable discovery catalog
resolve(catalog, discovery_id, expected_provider_binding) -> Result<Action, unavailable_or_stale>
```

Names/private helpers are implementation choices. No function performs I/O or dispatch. Do not take a raw natural-language query or payload parameter in Action construction or resolution. A future GUI consumer must separately enforce query origin, surface, protected-route admission and current execution authority; this package does not pretend these are enforced without a consumer.

### Producer → projection → materialization

1. Existing manager registration captures origin/name/epoch/declared capabilities. Its existing snapshot gives current revision and enabled state without provider locks.
2. The pure projection examines only those facts and the caller's current enabled-plugin/capability inputs. Do not call `Plugin::search`, `Plugin::commands`, `plugin_infos`, settings UI, a service, or a live store to populate descriptions.
3. For each of the eleven static descriptors, identify the owner by exact canonical name. If multiple entries share that name across any origin, mark it ambiguous and non-executable. A unique `DynamicLibrary` or `Registered` entry never acquires builtin annotations or executable eligibility by matching the name.
4. Require the unique owner to be Builtin, present, snapshot-enabled, allowed by the current explicit enabled-plugin set when present, declaring `search`, and allowed by the current enabled-capability map. Match current search restriction semantics: an absent per-plugin restriction is unrestricted; a present list without `search`, including an empty list, denies discovery. This policy is for discovery; it does not alter the special canonical `g ` route.
5. Emit one deterministic descriptor per stable ID in stable ID order, including a typed unavailable reason for missing/ambiguous/disabled/unsupported owners. Only eligible entries can materialize an Action. This preserves explanatory metadata without turning unavailable capabilities into executable suggestions.
6. Materialization calls only the pure fixed opener constructor belonging to the plugin. JSON uses the existing typed intent; all other constructors are zero-argument. It returns exact current label/description/action/args, without modifying the existing command shortcut factory.

The module must not retain provider objects, locks, dynamic library handles, callbacks, paths or configuration values. Existing snapshots are immutable; returned bindings identify owner origin/name/epoch and snapshot revision. Include a value-comparable eligibility stamp for the relevant enabled/capability policy, or perform direct comparison against current policy during resolution; do not rely on an unverified hash as authorization. A changed revision/epoch or eligibility invalidates a saved binding conservatively. An unrelated revision may require a harmless refresh; it must never silently rebind an old selected descriptor.

### State and adverse sequences

The only states are immutable **available** or **unavailable with reason**, plus resolution failure for stale identity. There is no pending worker, cancellation or retirement lifecycle.

- A builtin is disabled after projection: rebuild from the current snapshot/policy; resolution of the old binding fails. No Action is emitted for execution.
- A provider is replaced under the same name: a new epoch invalidates the old binding; matching text cannot transfer authority.
- A dynamic provider registers the builtin's name: ambiguous ownership fails closed even if one entry is builtin. This follows the existing Settings catalog's defensive precedent.
- A configured capability entry changes to an empty list: eligibility becomes false even when the source's declared capability remains `search`.
- A descriptor alias changes: stable descriptor identity and exact opening Action stay unchanged; neither existing command order nor usage keys are rewritten.

### Bounds, persistence, UI and concurrency

The shipped set is exactly eleven descriptors. Curated aliases are static, at most eight nonempty phrases per descriptor and at most 64 UTF-8 bytes each; keep the entire static discovery text below 32 KiB. If a proposed annotation exceeds those limits, revise its wording or defer the expansion; do not truncate user strings because no user strings belong in this catalog. Source-review the limits rather than claim benchmarked memory usage.

Projection visits the supplied plugin facts and a constant eleven seeds, with deterministic output order independent of map iteration. It runs only when a future caller requests it; this package adds no frame-time loop or background task. Avoid unnecessary cloning of unrelated provider data. Do not add an extra global cache; later consumers can cache by the existing revision plus current eligibility policy.

No persistence, migration, telemetry, query logging, schema version in user settings, UI change, keyboard change or new synchronization primitive is required. Opening functions may preserve existing dialog drafts and modes; this package does not clear them or claim that an existing dialog has no internal work. The guarantee is that catalog construction/resolution does not open the dialog, load content or execute any operation.

### Migration and rollback

First extract the existing Action construction into pure plugin-owned constructors, preserving all fields and matching rules. Reuse those constructors from the existing `search()` branches. Leave `commands()` shortcuts exactly as they are. Then add the typed static descriptors and projection/resolution API; do not wire the catalog into canonical search or GUI state. Add focused tests and source references.

This deliberately adds a new metadata API without a user-visible consumer; its approved consumer is a later separately packaged discovery feature. It is not a second executable path. If that feature is abandoned, the catalog can be removed without data migration; the small pure opener extractions may be retained only if independently useful. To roll back this package, revert its catalog/module/tests and restore the equivalent inline Action construction, preserving subsequent unrelated edits. Do not reset the repository or discard user changes.

## 5. Alternatives and rejection conditions

- **Documentation only:** lowest code risk and sufficient if no discovery implementation is selected. Prefer this if the user only wants research or if a current equivalent catalog already exists. It does not provide a tested typed metadata/materialization boundary.
- **Copy Action wire strings into a standalone discovery table:** initially small, but creates a second payload owner and lets query shortcuts be confused with opening actions. Reject for production; use plugin-owned constructors.
- **Call `Plugin::search` or `commands()` to discover features:** reject. Search can have side effects, and command shortcuts are not a complete typed capability contract. It also acquires providers unnecessarily.
- **Extend the public Plugin trait or rebuild all catalogs/dispatch:** disproportionate to eleven builtin openers and risks dynamic-plugin compatibility. Reject in this package.
- **Selected narrow projection:** reuses manager metadata and source-owned Action factories, with pure test seams. Maintenance cost is curated vocabulary plus eleven explicit IDs. Expected value depends on a later consumer; no measured relevance claim is required for this internal foundation.

Reject or narrow the proposal if current source already provides an equivalent tested facility, a named opener starts data work from hidden payload input, or metadata availability cannot be represented without unsafe provider access. Preserve the finding and continue unaffected documentation; do not patch those wider issues under this package.

## 6. Invariants and acceptance behavior

1. Exactly the eleven approved descriptors exist, with unique stable IDs and no alternate executable strings in the metadata owner.
2. For every descriptor, its materialized Action equals the corresponding existing plugin search result, including label, description and `args: None`; it parses to the existing expected typed opening command. JSON aliases still select the same intent.
3. Existing supported/unsupported query matches and exact `commands()` outputs remain unchanged. No command cache or completion entries are added.
4. Missing, wrong-origin, duplicate-name, disabled or search-capability-denied owners cannot materialize an executable Action. Missing runtime capability-map entries retain current unrestricted semantics.
5. Stale revision/epoch/policy bindings fail closed; they never silently resolve to a different provider or changed action.
6. Catalog construction/resolution takes immutable facts and static constructors only. It invokes no provider/search/command enumeration, service, filesystem, clipboard, network, worker or dispatcher.
7. No canonical search, Macro zero/one/many, radial, dashboard, hotfolder, execution-source/surface or Universal Action behavior changes. This is established by the absence of production integration plus focused source/diff review, not a broad native campaign.
8. User configuration formats, default settings and private-data retention are unchanged. No new dependency, global state or plugin ABI member is introduced.
9. Metadata output order and unavailable reasons are deterministic for equal source/policy inputs. Unavailable explanatory entries remain non-executable.

All required focused proof must pass, or the package remains incomplete with a precise gap. No UI screenshot, prototype score or unrelated passing test substitutes for these invariants.

## 7. Fresh branch and checkpoints

After explicit future authorization, discover repository/worktree/AGENTS/current source state, resolve the then-current master locally and verify its intended remote relationship without blindly pulling or resetting. If the user specifies an accepted prerequisite tip, record it. Preserve dirty work and use an isolated worktree when needed. Use branch stem **`codex/discovery-catalog`**, adding a short collision suffix only when required. A confirmed interrupted run resumes its existing task branch after identity checks.

Execution documentation: **`docs/implementation/SEARCH-F001-built-in-discovery-metadata/`**. Copy the authorized contract and kickstart there as `GOAL.md` and `KICKSTART.md`, retaining research provenance, and maintain `GOAL.md` section 13. Role records/reviews and verification evidence live beneath that directory.

| Checkpoint | Objective | Owners/outputs | Focused acceptance | Commit subject |
|---|---|---|---|---|
| M0-A | Safe authorized bootstrap | Task docs, branch/base/worktree record, role records | Current source and scope reconciled; dirty work preserved; correct fresh/resumed branch | `docs(search): record SEARCH-F001 implementation baseline` |
| M1-A | Complete metadata foundation | `src/search_discovery/`, `src/lib.rs`, nine plugin opener extractions and relevant tests; verification record | Invariants 1–9 and selected tests; no live query integration | `feat(search): add typed builtin discovery metadata` |
| M2-A | Independent review and close | Focused reviewer report, any required bounded correction, final progress | Findings resolved or exact incomplete state; final first-party lint/diff/source checks | `docs(search): close SEARCH-F001 metadata review` |

M1-A handoff:

- **Objective/owner:** one pure typed metadata catalog backed by existing manager facts and plugin-owned fixed Action constructors.
- **Current facts:** plugin search and query shortcut Actions differ; snapshot already carries identity and declared capabilities; enabled capability policy remains separate; Settings catalog supplies an ambiguity precedent.
- **Required steps:** confirm nine `name()` values and eleven typed parse targets; extract constructors without semantic changes; add static vocabulary/IDs; implement pure availability and stale-binding checks; add focused metadata and compatibility tests; inspect all changes for accidental search/cache/GUI integration.
- **Invariants/non-goals:** section 6; no ranking, provider traits, dispatcher, complete-prefix registry, query parser or new UI.
- **Dependencies:** M0-A plus current equivalents of the named source seams; neither audit repair is required.
- **Tests:** section 8; preserve existing exact-query and shortcut assertions, add direct factory parity and adverse identity/capability cases.
- **Verification/done:** nonzero selected tests pass, exact wire outputs and boundaries reviewed, no accidental production consumer, coherent feature commit plus one normal push attempt.
- **Genuine uncertainty:** only whether the future base has already changed these owners/contracts. Ordinary helper names and Rust ownership mechanics belong to the implementer.

M2-A handoff:

- **Objective:** an independent bounded review of actual diff and tests, then truthful closure.
- **Scope/tasks:** inspect new module, modified constructors, snapshot/policy use, exact command output parity and absence of integration. Reviewer returns findings; sole writer corrects necessary defects within scope. Review never becomes a wider launcher audit.
- **Dependencies:** accepted M1-A. Re-run only affected tests after corrections and the final scoped lint when code changed.
- **Done:** no unresolved mandatory invariant, final diff/status clean for task scope, role independence/model identities recorded, final source SHA/push visibility and rollback documented. If required evidence is unavailable, close as incomplete rather than accepted.

Commit each coherent accepted checkpoint and attempt one normal non-force push. Preserve successful local commits after push failure and continue. No force-push, history rewrite, automatic merge, repeated network recovery loop or unrelated staging.

## 8. Targeted future verification — not run during research

Current package name is `multi_launcher` (`Cargo.toml:1–6`); new and existing unit tests belong to its library. Revalidate target/features and installed tool versions before running future commands. No feature or TOML change is planned.

| Behavior/risk | Test owner/case | Fixture/setup | Expected observation | Level/bound |
|---|---|---|---|---|
| Exact opener parity | New `search_discovery` tests; existing nine plugin test modules, particularly hash `hash_prefix_opens_the_typed_generator_command`, JSON `json_query_family_maps_to_general_format_alias_and_minify_intents`, table/dff/image/activity exact-query tests | Stateless plugin values or existing inert/default fixtures; approved query strings only | New factory/materialized Action equals existing expected full fields and typed parse result; every args is None | Pure unit; no dispatch |
| Shortcut and matching parity | Existing JSON `command_catalog_exposes_all_supported_queries`, hash `quick_help_exposes_a_single_hash_entry`, and directly affected plugin assertions | Explicit valid, case/whitespace and unsupported queries already tested | Same `commands()` order/count/action/args; unsupported queries remain empty | Pure unit; preserve meaningful assertions |
| Availability and ownership | New module tests | Hand-built `PluginCatalogSnapshot` values for missing/builtin/dynamic/registered/duplicate names, enabled flags and declared capabilities; borrowed policy sets/maps | Only unique eligible builtin owner is executable; absent map restriction allowed; empty/no-search list denied | Pure unit; no manager construction required |
| Stale resolution | New module tests | Same-name owner with new epoch/revision; changed capability/enablement policy | Saved binding rejected; fresh eligible binding returns same fixed opener | Pure unit |
| Determinism and bounds | New module tests | Permuted plugin facts/policy insertion order, identical facts, all eleven seeds | Same stable ID order/reasons; no duplicate ID; vocabulary limits respected | Pure unit |
| No provider calls/private data | API/diff review and focused synthetic manager test only if necessary | Projection receives plain snapshots; if manager test needed use `PluginManager::new_inert_for_test_at` and existing `register_builtin_for_settings_test`; counter/panic provider methods must not be invoked after snapshot setup | Catalog path cannot invoke search/commands or acquire provider handles; no live stores or native runtimes | Structural proof plus narrowly justified unit test; temp root auto-cleanup |
| No automatic-consumer change | Source/diff check of `gui/search.rs`, `gui/render.rs`, command/cache/completion consumers | No app fixture needed because no integration is allowed | No new call from canonical search/Macro/GUI; unchanged command outputs | Static review, not native proof |

Use one selected Nextest batch covering the new module and directly modified plugin tests. Before execution, list matching tests and verify a nonzero count and the intended names. A prospective filter is the union of `search_discovery::` and the nine `plugins::<owner>::` module prefixes; resolve the actual names from the installed Nextest listing rather than claiming guessed names exist. Existing command parser/handler tests need selection only if their behavior or source is changed; parse calls inside the new parity tests already prove the intended types.

Budget: one initial library-test compilation and selected Nextest batch; at most three focused correction reruns for failures introduced by this package. No separate broad build is required when selected tests compile the library. If a compile-only check is needed before test discovery, use `cargo check -p multi_launcher --lib` once and record why. After final code changes, run scoped first-party Clippy for this library (for example `cargo clippy -p multi_launcher --lib --no-deps`, after checking installed support), with one correction rerun if needed. Do not fix unrelated vendor or pre-existing warnings; distinguish them from new warnings. Check formatting only for touched Rust files using repository tooling without formatting unrelated files.

No full suite, benchmark, application launch or native/manual validation is required: no native or visible behavior is introduced. No live user data or global settings are read or changed. If an app fixture becomes genuinely necessary, disable Paste Sequences' next/cancel hotkeys before constructing it and use an isolated synthetic workspace; explain why a pure fixture was insufficient. Temporary fixtures are owned by the test and cleaned up by their existing lifecycle. Never stop unrelated processes or delete another task's files.

## 9. Failure handling and bounded continuation

Fix ordinary localized compile/test defects within the selected rerun allowance, retaining the first meaningful failure and exact subsequent commands. Do not weaken Action equality, shortcut, capability or stale-binding assertions to obtain a pass.

If source revalidation shows an unsafe hidden operation, missing metadata authority or conflicting existing implementation, record the exact blocker and either narrow to an approved equivalent or leave the package incomplete. Do not add a dispatcher, plugin ABI change, live provider probe or new framework to force completion.

If a required tool is unavailable or the verification allowance is exhausted, preserve code/docs and record the precise unverified invariant and smallest next action. Continue unaffected documentation/review. Do not install tools, repeatedly dispatch unavailable roles, renew budgets silently or claim acceptance from unrun tests. A future authorized extension of the budget must be explicit.

Rollback is source-only and local to this feature as described in section 4. No user-data rollback or configuration migration is involved. Publish a clearly labeled partial/stall report when Git-safe; never commit through a conflict or unsafe index.

## 10. Models and role records

Recommended future assignments are planner **`gpt-6-astra` / `xhigh`**, implementer **`gpt-6.1-sol` / `xhigh`**, reviewer **`gpt-6-astra` / `xhigh`**, and an orchestration role using an available supported model such as **`gpt-6.1-sol` / `high`**. This is bounded typed metadata and constructor extraction; the more expensive research-wide setup is not mandatory. A pinned planner/reviewer role may request Astra max instead; record actual configured controls and do not silently rewrite global configuration.

Use sequential planner→single writer→separate reviewer contexts. Read-only roles return Markdown to the writer. Record requested/configured/resolved-or-unverified model/effort, source/base, role identity, authorship/persistence and review independence in `roles/<role>/ROLE_RECORD.md`. Put planner notes, implementer verification and reviewer findings in their own role folders. If a preferred model/role is unavailable, make at most one supported recovery attempt, then use an authorized available fallback and label it. Parent self-review must not be called independent.

## 11. Progress and stalls

Maintain the execution copy of `docs/implementation/SEARCH-F001-built-in-discovery-metadata/GOAL.md`, section 13, at real transitions. Record current branch/worktree/base, last accepted checkpoint, changed owners, tests run/not run, current issue, next action, commit and actual push visibility. Do not rely on conversation alone.

A genuine blocked/paused/deferred boundary gets `stalls/<checkpoint>-<UTC>.md` in that directory with sanitized command/diagnostic, expected versus actual, preserved state, ownership/cleanup facts, remaining verification allowance, evidence links and exact safe resume instruction. The timestamp is generated at execution, not supplied by the user. A stall/documentation commit does not imply feature acceptance.

## 12. Final delivery and authorization boundary

Deliver the typed catalog and pure owner constructors, meaningful targeted tests, execution contract/progress, role records, verification evidence and final reviewer disposition. Report source/base/branch/checkpoint/final commit, exact push status, accepted invariants or incomplete gaps, changes to the nine owners, checks run/not run, limitations and rollback.

This package is complete only when section 6 and required focused proof pass. It adds no visible discovery feature; say so plainly. Production ranking, UI integration, dynamic-plugin support and audit fixes require their own selection and authorization. Preparing this pair does not execute it. No PR creation, merge, release, deployment, destructive data operation or external account change is part of this package without separate appropriate authorization.

## 13. Execution progress — mutable

**State:** `PROPOSED_NOT_AUTHORIZED`. No implementation branch, source edits, test run, implementation commit or review acceptance exists for this package.

| Checkpoint | State | Evidence |
|---|---|---|
| M0-A | pending | Await package selection/authorization and current-source revalidation |
| M1-A | pending | No source implementation or targeted verification performed |
| M2-A | pending | No independent implementation review or final lint performed |

**Next action:** after the user selects this package, follow the sibling Kickstart, revalidate the then-current source, and establish the safe M0-A baseline. Do not execute the research roadmap or another package automatically.
