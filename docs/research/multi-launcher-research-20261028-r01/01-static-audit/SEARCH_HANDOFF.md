# Search research handoff — Goal 01 / M3-A

Source baseline: `59156d96adcc73939c7171487a0cb582e3286587`. Evidence is static source/caller/test inspection, not application or benchmark results. This handoff carries current contracts into Goal02 Intelligent Search and Action Discovery; it does not start Goal02 or authorize generated follow-ons. Parent will create Goal02's own fresh branch from **Goal01's recorded closing documentation commit after review**, not from an arbitrary earlier milestone. The separate review and parent correction pass are complete; the exact closing commit is recorded by the parent after the M3-B commit, before Goal02 bootstrap.

Use [architecture/query flows](ARCHITECTURE_AND_DATA_FLOWS.md#m1-b--query-routing-ranking-and-activation), [input](INPUT_AND_UI_OWNERSHIP.md), [lifecycle](LIFECYCLE_AND_CONCURRENCY.md), [safety](PERSISTENCE_AND_FILE_SAFETY.md), [decisions](DECISION_RECORDS.md) and [coverage](COVERAGE_MATRIX.md) for detail. Named bounded flows are inspected; the 105-bucket inventory is not a claim that every provider/native backend is deeply reviewed.

## 1. Real owners and identity

| Concern | Current owner / boundary | Constraint for discovery/ranking research |
|---|---|---|
| Wire payload | `actions::Action { label, desc, action, args }`, serde model | Preserve action string **and optional args**; display label, list position and semantic candidate ID are not substitutes. Reconstructing a command from its label can lose authority/payload. [I1] |
| Query/order | `LauncherApp::{search, search_read_only_outcome}`, shared snapshot scoring with supplied provider result | Reuse/represent current route, inclusion, weighting, Pending and revision/catalog semantics. A new experiment may compare alternatives but must label deliberate behavior changes. [Q1] |
| Providers | `Plugin`, `PluginManager`, `PluginSearchSnapshot`, per-provider query plan/tickets | Trait query policy, prefixes, capabilities, dynamic implementation and result finality are different facts. Snapshot ownership does not make a stale result currently executable. [Q2] |
| Activation | `CommandInvocation`, parse/bus/host/outcome and UniversalAction facade | Candidate relevance/visibility does not grant execution capability, confirmation or unattended authorization. Source and surface are distinct policy inputs. [A1] |
| Saved references | History pins, radial target/resolver and unattended fingerprints | Persisted action+args/reference/config identity must survive labels, ranking changes and catalog replacement. Domain resolution/revalidation remains authoritative. [I2], [A2] |

Usage identity is deliberately narrower: count key is **`Action.action` only**, so two Actions differing only by args share the current count. History pins match action ID **and args**; their labels/descriptions/query are stored presentation/context. Do not silently migrate usage to pin/candidate identity or count labels as stable targets. [U1], [U2], [I2]

## 2. Existing routes and query grammar

| Input / boundary | Current behavior to preserve as baseline |
|---|---|
| Empty/whitespace browse | Command cache then catalog actions displayed with `app ` label prefix; original action/args retained. This branch is not nonempty scored provider search. [Q1] |
| `app` / `app ` route | Recognized case-insensitively from trimmed query; local catalog tail receives action-filter grammar. Ordinary unprefixed text does not automatically mean full local application search. Providers retain their own routing rules. [Q3] |
| `g ` route | Normal/snapshot route substitutes the provider set `{web_search}` for ordinary enabled-provider selection; capability settings still apply. Existing parity test includes an empty ordinary enabled list. Do not “fix” this exception with a blanket enabled-list assumption. [Q4] |
| Ordinary provider query | Split `kind:`/`id:` filters; gate enabled provider/search capability/prefix/global/always-search policy; per-provider plan decides text/filter behavior. Empty prefix lists and always-search are real provider contracts. [Q2] |
| Matching-prefix Literal provider | Only when original query head matches its declared prefix, pass original input and skip action filters for that provider. Literal declaration alone does not make unrelated query heads literal. QR can carry filter-looking payload, spaces, quotes and newlines. [Q2], [Q5] |
| Empty provider-return vector, non-`g` route | For a non-`g` query, an empty provider-return vector enters command-cache fallback under the current exact/fuzzy rules. The explicit `g ` branch returns without that fallback. A nonempty provider vector that becomes empty during exact post-filtering does not trigger another fallback pass. Cache construction respects ordinary plugin settings and label order. [Q6] |
| Dashboard selected-provider query | PluginHome and Pinned Query explicitly split filters, query selected provider, then filter/truncate. They do not inherit manager Literal policy automatically. Pinned Query rejects disabled configured engine. D03 preserves existing saved widget grammar. [D1], [D2] |

Decoded token `value` classifies grammar; `raw` carries retained token spelling. `split_action_filters` normalizes separator whitespace when joining, so “raw token preserved” is not “entire original query byte-identical.” [AUD-001](findings/AUD-001-terminal-query-escape.md) is a known source inconsistency for unconsumed odd terminal escape; matching Literal limits manager-level impact. Snippet-local no-tag preservation does not undo upstream manager reconstruction. Do not silently repair production grammar during Goal02 prototype work or label the inconsistency intended behavior. Track any experimental normalization separately from baseline.

## 3. Current scoring and ordering

Let `F = fuzzy_weight`, `U = usage_weight`, `n(a) = usage[action.action]` with absent count zero. Current scored result adds `n(a) × U` to its base and sorts descending by floating score, with **no additional explicit semantic tie key** in the comparator. Defaults are F=1, U=1. There is no source-backed semantic/learned relevance metric or measured ranking improvement from this audit. [Q6], [U1], [U3]

| Candidate class | Current base/inclusion rule |
|---|---|
| Local `app` with empty remaining tail | All passing action metadata filters, base zero. |
| Local exact (`match_exact` or F≤0) | Case-folded displayed label, description, action-string or alias match; alias contributes one, otherwise zero. |
| Local fuzzy | A label/description fuzzy hit is required; maximum matcher score × F. |
| Returned provider action, fuzzy | Maximum label/description fuzzy score against raw query; unmatched can contribute zero and still retain the returned resolved action. This differs from local admission. |
| Returned provider action, exact | Current exact post-filter with resolved-output exceptions for QR, selected note commands, Clipboard Modify and date outputs; query suggestions do not receive the bypass. |
| Command fallback | Existing exact metadata/alias or label/description fuzzy match only for non-`g` routes with an empty input provider vector; no second fallback after exact post-filtering, and none during capacity deferral. |

The shared root/radial path should remain a parity reference; empty browse, exact resolved outputs, explicit routes and provider-returned candidates must not be flattened into one generic text match. Goal02 can propose tie policy, new retrieval or semantic features, but must distinguish candidate recall from ranking and explicitly record changed precedence/filter/usage behavior. Static source weights are compatibility facts, not evaluation labels. [Q3], [Q6]

## 4. Pending, publication, invalidation and selection

`LauncherSearchState::Pending` can coexist with rows. It is set when provider pending, provider revision changes during the search, or result catalog versions are unstable. Capacity deferral can retain locally scored `app` rows while pending, but does not run command-cache fallback; other nonempty routes are row-empty. Failed-provider reopening is a separate path that explicitly calls the provider-result scorer and can use its non-`g` command fallback. [Q1] “Has one result” and “all providers final” are therefore different predicates. [P1]

`PluginSearchUpdates` owns per-source active/published/resolved-through tickets and generations; schedule-or-join and publication validate active ticket under its lock. Cancellation resolves matching ticket and notifies, not worker retirement. UI generation changes invalidate search; capacity-release refresh uses **current query**, guarded against restoring worker-captured stale text. Radial has its own preparation/config/lease/ticket/catalog checks before freezing and again dispatching/confirming. [P2], [P3]

Root `search()` clears `selected` on cached-return, same-query equal-result reuse **and** replacement. Retaining vector geometry is not preserving active selection. Deferred Enter/click activation clones the exact current Action/args, then dispatches that payload rather than refetching a changed list index. Radial selection additionally freezes/resolves saved target and current authority. A discovery proposal must state its own lifetime/invalidation policy; it cannot assume root search already preserves stable selection across reranks. [P3], [A1], [A2]

Normal nonradial activation is not a universal provider-provenance re-resolution hook. Do not infer a new global guard exists, or classify every current Action as stale-executable by absence. Domain capability/native identity and current source/surface policies still apply. If a proposed automatic execution needs stronger finality/identity, make that a deliberate design with producer/admission/consumer/rejection ownership, not a label-based convenience.

## 5. Execution source, surface and human authority

| Route | Current authority / response limit |
|---|---|
| Enter and click | Same normal parse/activation/outcome boundary, exact deferred Action and source. Destructive confirmation retains original invocation until approval; cancel consumes it without dispatch. |
| Dashboard | Widget supplies Action and optional query override; dispatcher accepts override only for commands that support it. Structured QR/tool payloads opt out; display/query text does not replace typed action data. |
| Universal Action sheet/context | Passed availability/safety and operation checked; secondary effects normalized. Surface/context and ActivationSource are separate concepts. |
| Radial | Lease/revision/catalog/config/duplicate checks, fresh reference resolution, confirmation and root/history policy. Stale selection can fall back to saved query rather than execute, under existing policy. |
| Macro command broker | Production Windows backend uses newer broker. GUI's instantaneous zero/one/many gives NoResults / activates one with Macro source / presents many. Backend considers Activated **or presented selection** success. It does not await Pending=false or guarantee completed effect. D04 NO_ACTION preserves that contract. |
| Compatibility Macro query broker | Exported and polled; first current result with Enter source, not newer zero/one/many/Macro policy. Bounded src/tests search found no production submit caller; external/dynamic absence is not proven, so API is not removed. |
| Headless/legacy facade | Canonical parse plus headless compatibility/rejection policies; sharing parser does not imply all GUI services or confirmation behavior exist. |
| Hot Folder unattended | Exact unique persisted target/fingerprint/selected availability and supported external process path; not arbitrary command execution. Macro eligibility checks enabled instructions over compiled dependency program/defaults; clipboard/interactive instructions refused, supported nonwaiting processes require strong enable confirmation. |

Detailed source/counterguards: [interaction routes](ARCHITECTURE_AND_DATA_FLOWS.md#cross-surface-authority), [D04](DECISION_RECORDS.md#d04--preserve-macro-command-handoff-and-exported-compatibility-route), [unattended eligibility](PERSISTENCE_AND_FILE_SAFETY.md#automation-authority-nested-execution-and-receipts). Production Macro factory/handler and unattended capability anchors are [M1], [M2], [H1]. “Discovered” or natural-language relevance is never an unattended enable authorization. A selected candidate ID does not substitute for capability, confirmation, namespace/adapter or native file proof.

Jump hints select/focus; completion does not itself execute. Exact viewport/owner/generation/epoch, modal/transient authority, paired text/press/release and fresh Enter remain input-owner contracts. Preserve them if discovery UI changes; do not introduce a second synthetic-input/focus workaround from static speculation. Actual native timing remains unverified. [Input report](INPUT_AND_UI_OWNERSHIP.md)

## 6. Usage, history and privacy — newly revalidated owners

| Store / owner | Exact identity and persistence | Retention/privacy consequence |
|---|---|---|
| `usage.rs` | `UsageEntry { action:String, count:u32 }`, `usage.json`; map key is action string. Typed load distinguishes malformed errors; save revalidates existing file, transaction-locks and writes **replaceably**. Repair read permits participate. | No time-decay/TTL, key-count cap or per-args split is supplied by inspected usage owner. Action strings may contain paths/payload identifiers; counts are not automatically safe public telemetry. |
| `gui/actions.rs::record_history_usage` | Captures full Action, raw query, source label and timestamp; records action-string usage delta through target-repair handoff/barrier. Increment saturates; busy repair routes counts into pending/deferred delta state. | A recorded activation/history outcome is not independent evidence of successful completed effect or ideal relevance. Do not train ground truth from it without a separately justified signal model. |
| `history.rs::append_history` | Process-local transaction rereads persisted history, prepends entry, trims to caller's configured **entry count**, saves replaceably, then publishes committed in-memory snapshot. Default app history limit 100. | History retains raw query, full Action including args, source/time. Count cap applies on append; this trace does not establish a load-time byte cap or TTL. Raw user content is unsuitable for synthetic research evidence. |
| History pins | `HistoryPin` stores action_id/label/desc/args/query/time; identity is action_id **and args**. Process-local update reloads current file and durably saves before returning committed pins. | Pins are explicit persisted intent, not equivalent to usage counts or a current executable capability. Labels/context may be stale; semantic execution still needs target resolution. |
| Target repair usage handoff | Action-string deltas use saturating add under handoff/barrier/generation discipline. Exit stops/waits repairs, refuses unresolved handoff, applies ready reconciliation, then transaction-locks save. GUI fallback without client uses ordinary save. | Do not bypass this owner by overwriting usage from a prototype snapshot or relabel rekeyed identity during file repair. Full target-repair multi-store audit remains outside this targeted read. |

Pinned new evidence: [usage type/load/save](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/usage.rs#L1-L96), [history record](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/actions.rs#L315-L354), [history fields/identity](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/history.rs#L11-L82), [append/read snapshot](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/history.rs#L138-L174), [history replaceable commit](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/history.rs#L217-L226), [pin durable update](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/history.rs#L256-L290), [repair delta](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/target_repair.rs#L544-L578), [repair exit save](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/target_repair.rs#L2612-L2659), [GUI save caller](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L10112-L10124), [defaults](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings/defaults.rs#L33-L44).

Other data classes stay separate: clipboard history stores raw strings (default count 20, not a byte cap); Clipboard Modify undo retains original/modified text; Paste run owns rendered text/input lease. Macro completed summaries retain safe metadata with explicit 100-run/4MiB/1024-row budgets, whereas full runtime snapshots/variables/store/recording data are different classes. Hot Folder receipts carry paths/native identities/hash/time/rule/target metadata; “metadata” is not privacy-free. Screenshot/pin pixels, OCR result text and explicit Regex preset samples/replacements are raw content; rendering or existing persistence does not authorize ingesting them into a discovery index or evaluation corpus. [Safety data classes](PERSISTENCE_AND_FILE_SAFETY.md#clipboard-paste-and-output-ownership), [summary retention](LIFECYCLE_AND_CONCURRENCY.md#retention-and-read-heavy-outputs)

Goal02 should use only its contract-authorized synthetic fixtures/prototype scope, carry complete Action/args and source labels in safe synthetic examples, and avoid opening live user stores. No telemetry/training/retention policy change is accepted here.

## 7. Settings, files and worker boundaries that discovery cannot bypass

Provider capability/enabled settings are live service state with existing owned SaveIntent/prepared ticket/atomic commit/authoritative apply/rollback reconciliation. A new ranking config must not become a second independent source of eligibility or overwrite an old editor snapshot. Generic atomic save does not replace the settings runtime protocol. [Safety settings trace](PERSISTENCE_AND_FILE_SAFETY.md#settings-transaction-draft-to-acknowledged-runtime)

An Action containing a path is not native file identity evidence. Rename/Replace require confirmed typed selection, opaque source/root/parent proof, held handles, exact approved output and journal/recovery roles; Diff has a different metadata/path contract under AUD-F002 investigation. Search cannot convert a relevant path result into permission to mutate or clean a scratch file. Unattended actions retain their stricter enable/capability/native evidence. [File flows](PERSISTENCE_AND_FILE_SAFETY.md#batch-rename-verified-preview-through-journaled-moves)

Result rejection and resource retirement differ. Index/File Search/provider tickets may invalidate stale output without proving every worker/native call stopped; utility sessions have specific admission/joins/reaper/budget receipts. No universal process-wide cancellation latency or memory bound was measured. A new async discovery worker needs its own bounded admission/current-result/replacement/cancel/terminal/retirement/cleanup contract; reuse existing owner where appropriate, do not introduce a generic scheduler by default. [Lifecycle comparison](LIFECYCLE_AND_CONCURRENCY.md#owner-comparison)

## 8. Explicit change boundaries and dependencies

| Goal02 may research under its own contract | Must remain explicit rather than silently changing baseline |
|---|---|
| Alternative retrieval/ranking/features and synthetic relevance evaluation | Explicit app/g route behavior, Literal/filter payload, local/provider admission, resolved-output exemptions, usage key/weight and tie policy changes. |
| Better explanatory/action discovery presentation | Exact Action/args/target identity, current capability/availability, confirmation, source/surface and input/modal ownership. |
| Async provider handling proposals | Pending versus rows, revision/catalog/capacity fencing, selection invalidation, Macro handoff compatibility and worker retirement evidence. |
| Future metadata/persistence design | Schema/default migration, saved widget grammar/pins/references, settings field ownership, raw-content privacy and retention. |

AUD-F001 is an independent ready fix, not a prerequisite for research; Goal02 records the current raw-token issue and experimental differences without executing it. AUD-F002 is an independent decision/proof spike for file authority, not a blocker to pure synthetic ranking research and not a license to execute file mutations. [Finite catalog](FOLLOW_ON_INDEX.md)

Do not execute these packages during Goal02. Every Goal02 actionable recommendation still needs its own complete or precisely deduplicated Goal/Kickstart, source ownership, migration, invariants, dependencies, focused future proof and acceptance. Reuse existing canonical audit package IDs for the same mechanism; no duplicate tokenizer/filesystem framework proposals.

Remaining evidence limits: no current relevance/latency/RSS/build-cost measurement, no native focus/file/capture experiment, no full every-provider/algorithm audit, and no universal external-writer/atomicity guarantee. These limits constrain claims; they do not block bounded Goal02 research. The separate [review](roles/reviewer/REVIEW.md) required the fallback and snippet-boundary clarifications now applied; [parent disposition](REVIEW_AND_LIMITATIONS.md) records closure limits.

## Pinned existing-flow references

[I1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/actions/mod.rs#L12-L19
[I2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/history.rs#L28-L82
[Q1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1395-L1498
[Q2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1331-L1437
[Q3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L989-L1089
[Q4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L3910-L3948
[Q5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/qr.rs#L6-L68
[Q6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1125-L1392
[P1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1595-L1622
[P2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L205-L336
[P3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L817-L985
[U1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1671-L1676
[U2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/usage.rs#L10-L34
[U3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings/defaults.rs#L33-L44
[D1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/dashboard/widgets/pinned_query_results.rs#L384-L418
[D2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/dashboard/widgets/plugin_home.rs#L47-L54
[A1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/actions.rs#L69-L187
[A2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/radial_actions.rs#L2058-L2298
[M1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/mkmacro/executor.rs#L593-L637
[M2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L3095-L3191
[H1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/hotfolder/capability.rs#L259-L495
